//! Déroulé d'une partie : lancer l'arbitre (n'importe quel programme qui respecte le contrat),
//! attendre qu'il écoute, lancer le rouge, attendre sa connexion, lancer le noir, lire le résultat.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use thiserror::Error;

use isolation_core::board::{Color, Move};
use isolation_core::digest::short_hash;
use isolation_core::notation;
use isolation_core::record::{
    GameRecord, GameResult, Player, Reason, RefereeEvent, TournamentGame, ValidationError, now_rfc3339,
};

use crate::command::Vars;
use crate::config::Config;
use crate::hub::{self, Hub};
use crate::remote::{GameSpec, Seat, Seated};
use crate::roster::{Team, multithread_game};
use crate::store::compact_timestamp;
use crate::sync::lock;

/// Au-delà, le journal d'un joueur est tronqué : un jar ne doit pas pouvoir remplir le disque.
const LOG_CAP: usize = 1 << 20;
const MARKER_TIMEOUT: Duration = Duration::from_secs(60);
const LOG_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Error)]
pub enum GameError {
    #[error("lancement de {who} : {error}")]
    Spawn { who: String, error: io::Error },
    #[error("arbitre : `{0}` jamais reçu")]
    MarkerNeverSeen(String),
    #[error("partie interrompue après {0:?}")]
    Timeout(Duration),
    #[error("attente de l'arbitre : {0}")]
    Wait(io::Error),
    #[error("arbitre terminé avec {0}")]
    RefereeFailed(ExitStatus),
    #[error("résultat illisible ({error}) : {json:?}")]
    BadResult { json: String, error: serde_json::Error },
    #[error("résultat incohérent : {0}")]
    Inconsistent(#[from] ValidationError),
    /// Partie de tournoi avant son dernier essai (`Fixture::forfeit_counts`).
    #[error("{0} jamais connecté à l'arbitre, partie rejouée")]
    NoShow(String),
}

/// Partie de tournoi : son temps par coup et sa session remplacent ceux du ladder.
pub struct Fixture {
    pub game: TournamentGame,
    pub time_ms: u64,
    /// Faux : un `no_show` annule la partie (`GameError::NoShow`) au lieu de la donner à l'adversaire.
    pub forfeit_counts: bool,
}

pub struct Sigterm(#[cfg(unix)] signal_hook::iterator::Signals);

impl Sigterm {
    #[cfg(unix)]
    pub fn listen() -> io::Result<Self> {
        signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM]).map(Self)
    }

    #[cfg(not(unix))]
    #[expect(clippy::unnecessary_wraps, reason = "même signature que sous Unix")]
    pub fn listen() -> io::Result<Self> {
        Ok(Self())
    }
}

/// Joue au plus une partie par slot, ladder et smoke tests confondus : pas de port ni de coeur partagé.
pub struct Arena {
    cfg: Config,
    free: Mutex<Vec<usize>>,
    freed: Condvar,
    /// Levé par SIGTERM : plus de nouvelle partie, on finit celles en cours.
    draining: AtomicBool,
    smoke_tests: AtomicUsize,
    hub: Arc<Hub>,
}

impl Arena {
    pub fn new(cfg: Config, hub: Arc<Hub>) -> Self {
        let free = (0..cfg.slots()).rev().collect();
        Self {
            cfg,
            free: Mutex::new(free),
            freed: Condvar::new(),
            draining: AtomicBool::new(false),
            smoke_tests: AtomicUsize::new(0),
            hub,
        }
    }

    pub const fn config(&self) -> &Config {
        &self.cfg
    }

    #[cfg(unix)]
    pub fn drain_on(self: &Arc<Self>, mut sigterm: Sigterm) {
        let arena = Arc::clone(self);
        thread::spawn(move || {
            for _ in sigterm.0.forever() {
                arena.drain();
            }
        });
    }

    #[cfg(not(unix))]
    #[expect(clippy::unused_self, reason = "même signature que sous Unix")]
    pub fn drain_on(self: &Arc<Self>, _: Sigterm) {}

    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn drain(&self) {
        if !self.draining.swap(true, Ordering::SeqCst) {
            eprintln!("arrêt demandé : fin des parties en cours, aucune nouvelle");
        }
    }

    pub fn draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }

    /// Réserve la place d'un smoke test, refusée pendant l'arrêt. Compter avant de lire `draining` :
    /// la boucle, qui lit dans l'ordre inverse, ne peut pas sortir sans voir ce smoke test.
    pub fn hold(&self) -> Option<Hold<'_>> {
        self.smoke_tests.fetch_add(1, Ordering::SeqCst);
        let hold = Hold(self);
        (!self.draining()).then_some(hold)
    }

    pub fn smoke_tests(&self) -> usize {
        self.smoke_tests.load(Ordering::SeqCst)
    }

    /// Attend un slot libre de ce genre, celui de plus petit index.
    fn take_slot(&self, multithread: bool) -> Slot<'_> {
        let fits = |&index: &usize| self.cfg.multithread_slot(index) == multithread;
        let free = lock(&self.free);
        let mut free = self
            .freed
            .wait_while(free, |free| !free.iter().any(fits))
            .unwrap_or_else(PoisonError::into_inner);
        let Some(position) = free.iter().rposition(fits) else {
            unreachable!("attendu tant qu'aucun slot de ce genre n'est libre");
        };
        Slot {
            arena: self,
            index: free.remove(position),
        }
    }

    /// Ramasse les restes d'un arrêt brutal : `cleanup` avec `{game}` vide.
    pub fn sweep(&self) {
        let vars = Vars {
            cfg: &self.cfg,
            slot: 0,
            opening: String::new(),
            game: "",
            time_ms: self.cfg.time_ms,
        };
        drop(CleanupGuard::new(&vars));
    }

    pub fn play(
        &self,
        red: &Team,
        black: &Team,
        opening: &[Move],
        fixture: Option<&Fixture>,
        live: bool,
    ) -> Result<GameRecord, GameError> {
        let cfg = &self.cfg;
        let time_ms = fixture.map_or(cfg.time_ms, |f| f.time_ms);
        let slot = self.take_slot(multithread_game(cfg, red, black));
        let ts = now_rfc3339();
        let id = format!("{}-{}-{}", compact_timestamp(&ts), red.id, black.id);
        let entry = |team: &Team| Player {
            team: team.id.clone(),
            build: team.remote.as_ref().map_or_else(|| build_hash(team), Seat::build),
            anchor: team.anchor,
            remote: team.remote.is_some(),
        };
        let (red_entry, black_entry) = (entry(red), entry(black));
        let vars = Vars {
            cfg,
            slot: slot.index,
            opening: notation::join(opening),
            game: &id,
            time_ms,
        };
        let _cleanup = CleanupGuard::new(&vars);
        let mut on_air = live.then(|| {
            self.hub.start(&hub::Start {
                id: &id,
                red: &red.id,
                black: &black.id,
                since: &ts,
                time_ms,
                opening,
                tournament: fixture.map(|f| &f.game),
            });
            OnAir {
                hub: Arc::clone(&self.hub),
                id: id.clone(),
                result: None,
            }
        });
        let referee = RefereeProcess::spawn(&vars, on_air.as_ref().map(|o| Arc::clone(&o.hub)))?;

        let startup = Duration::from_millis(cfg.startup_ms);
        let spawn_player = |team: &Team, side: Color| {
            let log = cfg.player_log(&id, &team.id);
            if let Some(seat) = &team.remote {
                let connect = cfg.remote.as_ref().map(|r| vars.substitute(&r.connect, Some(side)));
                return Ok(Contestant::Remote(seat.sit(GameSpec {
                    id: id.clone(),
                    side,
                    time_ms,
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
            players.push(spawn_player(red, Color::Red)?);
            if referee.wait_for(&cfg.red_connected_marker, startup)? == Marker::Seen {
                players.push(spawn_player(black, Color::Black)?);
            }
        }
        let result = referee.finish(cfg.game_timeout(time_ms))?;
        drop(players);
        if let Some(absent) = annulled_no_show(&result, fixture, red, black) {
            return Err(GameError::NoShow(absent.id.clone()));
        }
        if let Some(on_air) = &mut on_air {
            on_air.result = Some((result.winner, result.reason));
        }

        Ok(GameRecord {
            id,
            session: fixture.map_or_else(|| cfg.session.clone(), |f| f.game.id.clone()),
            ts,
            red: red_entry,
            black: black_entry,
            tournament: fixture.map(|f| f.game.clone()),
            node: None,
            result,
        })
    }
}

struct OnAir {
    hub: Arc<Hub>,
    id: String,
    result: Option<(Color, Reason)>,
}

impl Drop for OnAir {
    fn drop(&mut self) {
        self.hub.end(&self.id, self.result);
    }
}

struct Slot<'a> {
    arena: &'a Arena,
    index: usize,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut free = lock(&self.arena.free);
        free.push(self.index);
        free.sort_unstable_by(|a, b| b.cmp(a));
        self.arena.freed.notify_all();
    }
}

pub struct Hold<'a>(&'a Arena);

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.0.smoke_tests.fetch_sub(1, Ordering::SeqCst);
    }
}

/// L'absent d'un `no_show` de tournoi qui n'a pas encore le droit de compter. Un jar figé a passé
/// le smoke test : s'il ne se connecte pas, le serveur (surchargé, trop chaud) est le premier suspect.
fn annulled_no_show<'t>(
    result: &GameResult,
    fixture: Option<&Fixture>,
    red: &'t Team,
    black: &'t Team,
) -> Option<&'t Team> {
    let counts = fixture.is_none_or(|f| f.forfeit_counts);
    (!counts && result.reason == Reason::NoShow).then(|| match result.winner.other() {
        Color::Red => red,
        Color::Black => black,
    })
}

fn build_hash(team: &Team) -> Option<String> {
    fs::read(team.build_file()?).ok().map(|bytes| short_hash(&bytes))
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
    fn spawn(vars: &Vars, hub: Option<Arc<Hub>>) -> Result<Self, GameError> {
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
            result: read_result(stdout, game_id, hub),
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

fn read_result(stdout: impl Read + Send + 'static, game: String, hub: Option<Arc<Hub>>) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut last = String::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            match classify(&line) {
                Line::Event(event) => {
                    if let Some(hub) = &hub {
                        hub.event(&game, event);
                    }
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
    Local(#[expect(dead_code, reason = "tenu pour son Drop")] LoggedPlayer),
    Remote(#[expect(dead_code, reason = "tenu pour son Drop")] Seated),
}

struct LoggedPlayer {
    process: Option<Reaper>,
    written: Receiver<()>,
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
mod tests {
    use super::*;

    #[test]
    fn a_game_waits_for_a_free_slot() {
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n[[slot]]\n[[slot]]\n"
            .parse()
            .unwrap();
        let arena = Arena::new(cfg, Hub::new(|_, _, _| {}));
        let first = arena.take_slot(false);
        let second = arena.take_slot(false);
        assert_eq!((first.index, second.index), (0, 1));
        thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let arena = &arena;
            scope.spawn(move || tx.send(arena.take_slot(false).index).unwrap());
            assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
            drop(second);
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(1));
        });
        drop(first);
        assert_eq!(arena.take_slot(false).index, 0);
    }

    #[test]
    fn a_multithread_game_waits_for_a_multithread_slot() {
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n\
                           [[slot]]\n[[slot]]\nmultithread = true\n[[slot]]\n"
            .parse()
            .unwrap();
        let arena = Arena::new(cfg, Hub::new(|_, _, _| {}));
        let multithread = arena.take_slot(true);
        assert_eq!(multithread.index, 1);
        assert_eq!(arena.take_slot(false).index, 0);
        thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let arena = &arena;
            scope.spawn(move || tx.send(arena.take_slot(true).index).unwrap());
            assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
            drop(multithread);
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(1));
        });
    }

    fn result_of(stdout: &'static str, hub: Option<Arc<Hub>>) -> String {
        read_result(stdout.as_bytes(), "g1".into(), hub)
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
    }

    #[test]
    fn events_go_live_and_the_result_is_the_first_other_json_line() {
        let hub = Hub::new(|_, _, _| {});
        hub.start(&hub::Start {
            id: "g1",
            red: "a",
            black: "b",
            since: "",
            time_ms: 100,
            opening: &[],
            tournament: None,
        });
        let mut watcher = hub.subscribe().unwrap();
        let stdout = "{\"event\":\"ready\"}\nbruit\n{\"event\":\"move\",\"move\":\"G1-G4-F9\",\"ms\":3}\n\
                      {\"event\":\"futur\"}\n{\"v\":1}\n{\"v\":2}\n";
        assert_eq!(result_of(stdout, Some(Arc::clone(&hub))), "{\"v\":1}");
        let moved: serde_json::Value = serde_json::from_str(&watcher.events.try_recv().unwrap()).unwrap();
        assert_eq!(moved["move"], "G1-G4-F9");
        assert!(watcher.events.try_recv().is_err());
    }

    #[test]
    fn a_referee_without_events_still_gives_its_result() {
        assert_eq!(result_of("{\"v\":1}\n", None), "{\"v\":1}");
    }

    #[test]
    fn without_a_result_the_last_line_is_kept_for_the_error() {
        assert_eq!(result_of("{\"event\":\"ready\"}\nplantage\n", None), "plantage");
        assert_eq!(result_of("", None), "");
    }

    #[cfg(unix)]
    #[test]
    fn a_sigterm_before_the_arena_exists_still_drains_it() {
        let sigterm = Sigterm::listen().unwrap();
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n".parse().unwrap();
        let arena = Arc::new(Arena::new(cfg, Hub::new(|_, _, _| {})));
        arena.drain_on(sigterm);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !arena.draining() {
            assert!(std::time::Instant::now() < deadline, "SIGTERM lost");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn draining_refuses_new_smoke_tests_and_keeps_counting_running_ones() {
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n".parse().unwrap();
        let arena = Arena::new(cfg, Hub::new(|_, _, _| {}));
        let running = arena.hold().unwrap();
        arena.drain();
        assert!(arena.draining());
        assert!(arena.hold().is_none());
        assert_eq!(arena.smoke_tests(), 1);
        drop(running);
        assert_eq!(arena.smoke_tests(), 0);
    }

    #[test]
    fn a_tournament_no_show_is_annulled_until_the_last_try() {
        let team = |id: &str| Team {
            id: id.into(),
            cmd: crate::command::CommandTemplate::default(),
            build: None,
            anchor: false,
            multithread: false,
            remote: None,
        };
        let (red, black) = (team("eq01"), team("eq02"));
        let result = |winner, reason| GameResult {
            v: isolation_core::record::RECORD_VERSION,
            time_ms: 5000,
            opening: vec![],
            moves: vec![],
            winner,
            reason,
            invalid: isolation_core::record::InvalidCounts::default(),
            duration_ms: 10_000,
            move_ms: vec![],
        };
        let fixture = |forfeit_counts| Fixture {
            game: TournamentGame {
                id: "tournoi-2026-11-08".into(),
                match_id: "W1".into(),
                game: 1,
            },
            time_ms: 5000,
            forfeit_counts,
        };
        let absent = |r: &GameResult, f: Option<&Fixture>| annulled_no_show(r, f, &red, &black).map(|t| t.id.clone());
        let black_absent = result(Color::Red, Reason::NoShow);
        assert_eq!(absent(&black_absent, Some(&fixture(false))).as_deref(), Some("eq02"));
        assert_eq!(
            absent(&result(Color::Black, Reason::NoShow), Some(&fixture(false))).as_deref(),
            Some("eq01")
        );
        assert_eq!(absent(&black_absent, Some(&fixture(true))), None);
        assert_eq!(absent(&black_absent, None), None);
        assert_eq!(
            absent(&result(Color::Red, Reason::Timeout), Some(&fixture(false))),
            None
        );
    }
}
