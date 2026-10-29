//! Déroulé d'une partie : lancer l'arbitre (n'importe quel programme qui respecte le contrat),
//! attendre qu'il écoute, lancer le rouge, attendre sa connexion, lancer le noir, lire le résultat.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Condvar, Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use thiserror::Error;

use isolation::board::{Color, Move};
use isolation::notation;
use isolation::record::{GameRecord, GameResult, Player, ValidationError, now_rfc3339};

use crate::config::{Config, Team, Vars};
use crate::digest::short_hash;
use crate::journal::compact_timestamp;

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
}

/// Joue au plus une partie par slot, ladder et smoke tests confondus : pas de port ni de coeur partagé.
pub struct Arena {
    cfg: Config,
    free: Mutex<Vec<usize>>,
    freed: Condvar,
}

impl Arena {
    pub fn new(cfg: Config) -> Self {
        let free = (0..cfg.slots()).rev().collect();
        Self {
            cfg,
            free: Mutex::new(free),
            freed: Condvar::new(),
        }
    }

    pub const fn config(&self) -> &Config {
        &self.cfg
    }

    /// Attend un slot libre de ce genre, celui de plus petit index.
    fn take_slot(&self, multithread: bool) -> Slot<'_> {
        let fits = |&index: &usize| self.cfg.multithread_slot(index) == multithread;
        let free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
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
        };
        drop(CleanupGuard::new(&vars));
    }

    pub fn play(&self, red: &Team, black: &Team, opening: &[Move]) -> Result<GameRecord, GameError> {
        let cfg = &self.cfg;
        let slot = self.take_slot(cfg.multithread_game(red, black));
        let ts = now_rfc3339();
        let id = format!("{}-{}-{}", compact_timestamp(&ts), red.id, black.id);
        // Avant la partie : un smoke test peut remplacer le jar pendant qu'elle se joue.
        let entry = |team: &Team| Player {
            team: team.id.clone(),
            build: build_hash(team),
            anchor: team.anchor,
            remote: false,
        };
        let (red_entry, black_entry) = (entry(red), entry(black));
        let vars = Vars {
            cfg,
            slot: slot.index,
            opening: notation::join(opening),
            game: &id,
        };
        // Déclaré avant les processus : détruit après eux.
        let _cleanup = CleanupGuard::new(&vars);
        let referee = RefereeProcess::spawn(&vars)?;

        let startup = Duration::from_millis(cfg.startup_ms);
        let spawn_player = |team: &Team, side: Color| {
            let log = cfg.player_log(&id, &team.id);
            vars.command(&team.cmd, Some(side))
                .and_then(|command| spawn_logged(command, &log))
                .map_err(|error| GameError::Spawn {
                    who: team.id.clone(),
                    error,
                })
        };
        // Si l'arbitre se termine avant un marqueur, un joueur manque : son résultat est déjà rendu.
        let mut players = Vec::with_capacity(2);
        if referee.wait_for(&cfg.ready_marker, startup)? == Marker::Seen {
            players.push(spawn_player(red, Color::Red)?);
            if referee.wait_for(&cfg.red_connected_marker, startup)? == Marker::Seen {
                players.push(spawn_player(black, Color::Black)?);
            }
        }
        let result = referee.finish(cfg.game_timeout())?;
        drop(players);

        Ok(GameRecord {
            id,
            session: cfg.session.clone(),
            ts,
            red: red_entry,
            black: black_entry,
            result,
        })
    }
}

/// Rendu à la sortie de portée, après le nettoyage de la partie.
struct Slot<'a> {
    arena: &'a Arena,
    index: usize,
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        let mut free = self.arena.free.lock().unwrap_or_else(PoisonError::into_inner);
        free.push(self.index);
        free.sort_unstable_by(|a, b| b.cmp(a));
        self.arena.freed.notify_all();
    }
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
    /// Lignes du stderr de l'arbitre, déjà relayées sur le nôtre.
    lines: Receiver<String>,
    /// Première ligne de son stdout : le résultat JSON.
    result: Receiver<String>,
}

impl RefereeProcess {
    fn spawn(vars: &Vars) -> Result<Self, GameError> {
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
            result: read_first_line(stdout),
        })
    }

    /// Sans marqueur, on attend simplement `startup`.
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

/// Lu dans un thread, pour pouvoir abandonner une partie qui ne finit jamais.
fn read_first_line(stdout: ChildStdout) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    rx
}

/// Tue le processus à la sortie de portée, et ses enfants directs : un script d'enrobage ne doit
/// pas laisser d'orphelins. Tout reste dans notre groupe de processus pour que Ctrl+C les atteigne.
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

/// Lance `cleanup` à la sortie de portée.
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

/// Joueur dont la sortie d'erreur va dans un journal.
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

/// Joueur dont la sortie d'erreur va dans `log`, tronquée à `LOG_CAP` octets.
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
        // On lit jusqu'au bout, même après le plafond, pour ne pas bloquer le joueur sur un tube plein.
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
        let arena = Arena::new(cfg);
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
        let arena = Arena::new(cfg);
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
}
