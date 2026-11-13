//! Déroulé d'une partie sur une machine : lancer l'arbitre (n'importe quel programme qui respecte le
//! contrat), attendre qu'il écoute, lancer le rouge, attendre sa connexion, lancer le noir, lire le
//! résultat. [`Local`] le fait ici, au serveur principal comme sur un nœud ; le ladder confie ses
//! parties à un [`Executor`] et ne voit que le résultat, le direct et les journaux.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use isolation_core::board::{ByColor, Color, Move};
use isolation_core::notation;
use isolation_core::record::{GameResult, Reason, RefereeEvent};

use crate::arena::GameError;
use crate::command::{ExecConfig, Vars};
use crate::remote::{GameSpec, Seated};
use crate::roster::Team;

/// Une partie à jouer.
pub struct GameRun<'a> {
    pub id: &'a str,
    /// Index du slot sur la machine qui joue : ses cœurs et ses remplacements de `[sandbox]`.
    pub slot: usize,
    pub time_ms: u64,
    pub opening: &'a [Move],
    pub red: &'a Team,
    pub black: &'a Team,
    pub timeout: Duration,
}

/// Ce que la partie produit en chemin, en plus de son résultat.
pub trait GameOutput: Send + Sync {
    /// Un événement de l'arbitre (prêt, coup) au moment où il arrive.
    fn event(&self, event: RefereeEvent);
    /// Fichier de la sortie d'erreur du joueur `side`.
    fn log_path(&self, side: Color) -> PathBuf;
}

/// Joue une partie quelque part.
pub trait Executor: Send + Sync {
    fn run(&self, run: &GameRun<'_>, out: Arc<dyn GameOutput>) -> Result<GameResult, GameError>;
    /// Arrête la partie `game` en cours : son `run` rend vite une erreur.
    fn abort(&self, game: &str);
}

/// Joue sur cette machine, dans ses bacs à sable.
pub struct Local {
    cfg: ExecConfig,
}

impl Local {
    pub const fn new(cfg: ExecConfig) -> Self {
        Self { cfg }
    }

    /// Ramasse les restes d'un arrêt brutal : `cleanup` avec `{game}` vide.
    pub fn sweep(&self) {
        self.abort("");
    }
}

impl Executor for Local {
    /// `cleanup` de la partie : ses conteneurs tués, l'arbitre sort et `run` rend la main.
    fn abort(&self, game: &str) {
        let vars = Vars {
            cfg: &self.cfg,
            slot: 0,
            opening: String::new(),
            game,
            time_ms: 0,
        };
        drop(CleanupGuard::new(&vars));
    }

    fn run(&self, run: &GameRun<'_>, out: Arc<dyn GameOutput>) -> Result<GameResult, GameError> {
        let cfg = &self.cfg;
        let vars = Vars {
            cfg,
            slot: run.slot,
            opening: notation::join(run.opening),
            game: run.id,
            time_ms: run.time_ms,
        };
        let _cleanup = CleanupGuard::new(&vars);
        let referee = RefereeProcess::spawn(&vars, Arc::clone(&out))?;

        let startup = Duration::from_millis(cfg.startup_ms);
        let spawn_player = |team: &Team, side: Color| {
            let log = out.log_path(side);
            if let Some(seat) = &team.remote {
                let connect = cfg.remote_connect.as_ref().map(|c| vars.substitute(c, Some(side)));
                return Ok(Contestant::Remote(seat.sit(GameSpec {
                    id: run.id.to_string(),
                    side,
                    time_ms: run.time_ms,
                    connect: connect.unwrap_or_default(),
                    log,
                })));
            }
            vars.command(&team.cmd, Some(side))
                .and_then(|command| spawn_logged(command, &log))
                .map(Contestant::Local)
                .map_err(|error| GameError::Spawn {
                    who: team.id.clone(),
                    error,
                })
        };
        let mut players = Vec::with_capacity(2);
        if referee.wait_for(&cfg.ready_marker, startup)? == Marker::Seen {
            players.push(spawn_player(run.red, Color::Red)?);
            if referee.wait_for(&cfg.red_connected_marker, startup)? == Marker::Seen {
                players.push(spawn_player(run.black, Color::Black)?);
            }
        }
        let result = referee.finish(run.timeout)?;
        let mut exits = ByColor::default();
        // Seul un absent peut être une panne ; les autres parties n'attendent pas leurs joueurs.
        if result.reason == Reason::NoShow {
            for (player, side) in players.iter_mut().zip([Color::Red, Color::Black]) {
                if let Contestant::Local(player) = player {
                    exits[side] = player.exit_code(EXIT_GRACE);
                }
            }
        }
        drop(players);
        if let Some((side, code)) = infra_failure(&result, exits) {
            return Err(GameError::Infra { side, code });
        }
        Ok(result)
    }
}

/// Au-delà, le journal d'un joueur est tronqué : un jar ne doit pas pouvoir remplir le disque.
pub const LOG_CAP: usize = 1 << 20;
const MARKER_TIMEOUT: Duration = Duration::from_secs(60);
const LOG_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);
/// Le client `docker run` d'un joueur absent a eu le temps de rendre son code de sortie.
const EXIT_GRACE: Duration = Duration::from_secs(2);

/// Codes de `docker run` quand le conteneur n'a pas pu tourner (125 : démon ou runtime, 126 :
/// commande non exécutable, 127 : commande introuvable). `player-entry.sh` ramène ceux du jar à 1 :
/// un jar ne peut pas se faire passer pour une panne.
const DOCKER_FAILED: std::ops::RangeInclusive<i32> = 125..=127;

/// Le joueur absent d'un `no_show` dont le bac à sable n'a jamais tourné, et son code. Un joueur qui
/// s'est connecté a perdu pour de vrai, quel que soit son code.
fn infra_failure(result: &GameResult, exits: ByColor<Option<i32>>) -> Option<(Color, i32)> {
    if result.reason != Reason::NoShow {
        return None;
    }
    let absent = result.winner.other();
    exits[absent]
        .filter(|code| DOCKER_FAILED.contains(code))
        .map(|code| (absent, code))
}

#[derive(PartialEq, Eq)]
enum Marker {
    Seen,
    RefereeExited,
}

struct RefereeProcess {
    process: Reaper,
    lines: Receiver<String>,
    result: Receiver<String>,
}

impl RefereeProcess {
    fn spawn(vars: &Vars, out: Arc<dyn GameOutput>) -> Result<Self, GameError> {
        let spawn_error = |error| GameError::Spawn {
            who: "l'arbitre".into(),
            error,
        };
        let mut child = vars
            .command(&vars.cfg.referee, None)
            .and_then(|mut c| c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn())
            .map_err(spawn_error)?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            unreachable!("tubes demandés au lancement");
        };

        let (line_tx, lines) = mpsc::channel();
        let game = vars.game.to_string();
        let game_id = game.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                eprintln!("   [{game}] {line}");
                if line_tx.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            process: Reaper(child),
            lines,
            result: read_result(stdout, game_id, out),
        })
    }

    fn wait_for(&self, marker: &str, startup: Duration) -> Result<Marker, GameError> {
        if marker.is_empty() {
            thread::sleep(startup);
            return Ok(Marker::Seen);
        }
        loop {
            match self.lines.recv_timeout(MARKER_TIMEOUT) {
                Ok(line) if line.trim().starts_with(marker) => return Ok(Marker::Seen),
                Ok(_) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(Marker::RefereeExited),
                Err(RecvTimeoutError::Timeout) => return Err(GameError::MarkerNeverSeen(marker.into())),
            }
        }
    }

    fn finish(mut self, timeout: Duration) -> Result<GameResult, GameError> {
        let json = self
            .result
            .recv_timeout(timeout)
            .map_err(|_| GameError::Timeout(timeout))?;
        let status = self.process.0.wait().map_err(GameError::Wait)?;
        if !status.success() {
            return Err(GameError::RefereeFailed(status));
        }
        let result: GameResult = serde_json::from_str(&json).map_err(|error| GameError::BadResult { json, error })?;
        result.validate()?;
        Ok(result)
    }
}

enum Line {
    Event(RefereeEvent),
    UnknownEvent,
    Result,
    NotJson,
}

fn classify(line: &str) -> Line {
    match serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(line) {
        Ok(object) if object.contains_key("event") => {
            serde_json::from_value(object.into()).map_or(Line::UnknownEvent, Line::Event)
        }
        Ok(_) => Line::Result,
        Err(_) => Line::NotJson,
    }
}

fn read_result(stdout: impl Read + Send + 'static, game: String, out: Arc<dyn GameOutput>) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut last = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match classify(&line) {
                Line::Event(event) => {
                    out.event(event);
                }
                Line::UnknownEvent => {}
                Line::Result => {
                    let _ = tx.send(line);
                    return;
                }
                Line::NotJson => {
                    eprintln!("   [{game}] stdout ignoré : {line}");
                    last = line;
                }
            }
        }
        let _ = tx.send(last);
    });
    rx
}

struct Reaper(Child);

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = Command::new("pkill")
            .args(["-KILL", "-P", &self.0.id().to_string()])
            .status();
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CleanupGuard(Option<Command>);

impl CleanupGuard {
    fn new(vars: &Vars) -> Self {
        let cleanup = &vars.cfg.cleanup;
        Self(
            (!cleanup.is_empty())
                .then(|| vars.command(cleanup, None).ok())
                .flatten(),
        )
    }
}

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        if let Some(command) = &mut self.0 {
            let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
}

enum Contestant {
    Local(LoggedPlayer),
    Remote(#[expect(dead_code, reason = "tenu pour son Drop")] Seated),
}

struct LoggedPlayer {
    process: Option<Reaper>,
    written: Receiver<()>,
}

impl LoggedPlayer {
    /// Code de sortie si le processus finit d'ici `grace` ; `None` s'il tourne encore ou a été tué
    /// par un signal.
    fn exit_code(&mut self, grace: Duration) -> Option<i32> {
        let child = &mut self.process.as_mut()?.0;
        let deadline = std::time::Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.code(),
                Ok(None) if std::time::Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                _ => return None,
            }
        }
    }
}

impl Drop for LoggedPlayer {
    fn drop(&mut self) {
        drop(self.process.take());
        let _ = self.written.recv_timeout(LOG_FLUSH_TIMEOUT);
    }
}

fn spawn_logged(mut command: Command, log: &Path) -> io::Result<LoggedPlayer> {
    let mut file = File::create(log)?;
    let mut child = command.stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    let Some(mut stderr) = child.stderr.take() else {
        unreachable!("tube demandé au lancement");
    };
    let (done, written) = mpsc::channel();
    thread::spawn(move || {
        let _done = done;
        let mut kept = 0;
        let mut buf = [0u8; 8192];
        while let Ok(n @ 1..) = stderr.read(&mut buf) {
            if kept < LOG_CAP {
                let take = n.min(LOG_CAP - kept);
                let _ = file.write_all(&buf[..take]);
                kept += take;
                if kept == LOG_CAP {
                    let _ = file.write_all("\n[journal tronqué]\n".as_bytes());
                }
            }
        }
    });
    Ok(LoggedPlayer {
        process: Some(Reaper(child)),
        written,
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// Garde les événements et range les journaux dans `dir`.
    #[derive(Default)]
    pub struct Seen {
        pub events: std::sync::Mutex<Vec<RefereeEvent>>,
        pub dir: PathBuf,
    }

    impl GameOutput for Seen {
        fn event(&self, event: RefereeEvent) {
            self.events.lock().unwrap().push(event);
        }

        fn log_path(&self, side: Color) -> PathBuf {
            self.dir.join(format!("{}.log", side.name()))
        }
    }

    fn result_of(stdout: &'static str) -> (String, Vec<RefereeEvent>) {
        let seen = Arc::new(Seen::default());
        let out: Arc<dyn GameOutput> = Arc::clone(&seen) as Arc<dyn GameOutput>;
        let result = read_result(stdout.as_bytes(), "g1".into(), out)
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        let events = seen.events.lock().unwrap().clone();
        (result, events)
    }

    fn team(id: &str, cmd: [&str; 3]) -> Team {
        Team {
            id: id.into(),
            cmd: crate::command::CommandTemplate::new(cmd),
            build: None,
            anchor: false,
            multithread: false,
            portable: false,
            remote: None,
        }
    }

    #[test]
    fn local_executor_runs_the_referee_then_both_players_and_returns_the_result() {
        let dir = std::env::temp_dir().join(format!("isolation-exec-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let result = r#"{"v":1,"time_ms":100,"opening":[],"moves":[],"winner":"black","reason":"timeout","invalid":{"red":0,"black":0},"duration_ms":5}"#;
        let script = format!(
            "echo LISTENING >&2; echo 'CONNECTED red' >&2; echo '{{\"event\":\"ready\"}}'; sleep 1; echo '{result}'"
        );
        let cfg: ExecConfig = toml::from_str(&format!("referee = [\"sh\", \"-c\", {script:?}]\n")).unwrap();
        let player = ["sh", "-c", "echo joueur-{side} >&2"];
        let (red, black) = (team("a", player), team("b", player));
        let seen = Arc::new(Seen {
            dir: dir.clone(),
            ..Seen::default()
        });
        let run = GameRun {
            id: "g1",
            slot: 0,
            time_ms: 100,
            opening: &[],
            red: &red,
            black: &black,
            timeout: Duration::from_secs(10),
        };
        let out = Arc::clone(&seen) as Arc<dyn GameOutput>;
        let game = Local::new(cfg).run(&run, out).unwrap();
        assert_eq!((game.winner, game.reason), (Color::Black, Reason::Timeout));
        assert_eq!(*seen.events.lock().unwrap(), [RefereeEvent::Ready]);
        for side in ["red", "black"] {
            let log = std::fs::read_to_string(dir.join(format!("{side}.log"))).unwrap();
            assert_eq!(log.trim(), format!("joueur-{side}"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exit_code_waits_a_little_for_a_finished_player_only() {
        let dir = std::env::temp_dir().join(format!("isolation-exit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spawn = |script: &str| {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            spawn_logged(command, &dir.join("joueur.log")).unwrap()
        };
        assert_eq!(spawn("exit 125").exit_code(Duration::from_secs(5)), Some(125));
        assert_eq!(spawn("exit 0").exit_code(Duration::from_secs(5)), Some(0));
        let started = std::time::Instant::now();
        assert_eq!(
            spawn("sleep 5").exit_code(Duration::from_millis(100)),
            None,
            "encore vivant"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "n'attend que le délai donné"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn events_go_out_and_the_result_is_the_first_other_json_line() {
        let stdout = "{\"event\":\"ready\"}\nbruit\n{\"event\":\"move\",\"move\":\"G1-G4-F9\",\"ms\":3}\n\
                      {\"event\":\"futur\"}\n{\"v\":1}\n{\"v\":2}\n";
        let (result, events) = result_of(stdout);
        assert_eq!(result, "{\"v\":1}");
        assert_eq!(
            events,
            [
                RefereeEvent::Ready,
                RefereeEvent::Move {
                    mv: "G1-G4-F9".into(),
                    ms: 3
                }
            ]
        );
    }

    #[test]
    fn a_referee_without_events_still_gives_its_result() {
        assert_eq!(result_of("{\"v\":1}\n").0, "{\"v\":1}");
    }

    #[test]
    fn without_a_result_the_last_line_is_kept_for_the_error() {
        assert_eq!(result_of("{\"event\":\"ready\"}\nplantage\n").0, "plantage");
        assert_eq!(result_of("").0, "");
    }

    #[test]
    fn an_absent_player_whose_docker_failed_is_an_infra_error() {
        let result = |winner, reason| GameResult {
            v: isolation_core::record::RECORD_VERSION,
            time_ms: 500,
            opening: vec![],
            moves: vec![],
            winner,
            reason,
            invalid: isolation_core::record::InvalidCounts::default(),
            duration_ms: 0,
            move_ms: vec![],
        };
        let exits = |red, black| ByColor { red, black };
        let black_absent = result(Color::Red, Reason::NoShow);
        assert_eq!(
            infra_failure(&black_absent, exits(None, Some(125))),
            Some((Color::Black, 125))
        );
        assert_eq!(infra_failure(&black_absent, exits(None, Some(1))), None);
        assert_eq!(infra_failure(&black_absent, exits(None, None)), None);
        let red_timed_out = result(Color::Black, Reason::Timeout);
        assert_eq!(infra_failure(&red_timed_out, exits(Some(125), None)), None);
        let red_absent = result(Color::Black, Reason::NoShow);
        assert_eq!(
            infra_failure(&red_absent, exits(Some(127), None)),
            Some((Color::Red, 127))
        );
    }
}
