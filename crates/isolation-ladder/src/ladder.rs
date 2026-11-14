//! Boucle principale : remplit les slots libres, puis traite les comptes rendus des threads de partie.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use anyhow::Context;
use rand::RngExt;
use rand::rngs::ThreadRng;

use isolation_core::record::GameRecord;

use crate::api;
use crate::arena::{Arena, GameError, Sigterm};
use crate::config::Config;
use crate::db::Db;
use crate::live::Live;
use crate::migrate::open_db;
use crate::pairs::Pairs;
use crate::play::{Report, live_hub, remote_registry};
use crate::remote::Registry;
use crate::roster::{Builds, portable_teams, roster};

use crate::schedule::Scheduler;
use crate::standings::Standings;
use crate::thermal::Thermal;
use crate::tournament::Organizer;

pub const IDLE_DELAY: Duration = Duration::from_secs(10);
const LOG_RETENTION: Duration = Duration::from_hours(60 * 24);

pub fn load(path: &Path) -> anyhow::Result<Config> {
    fs::read_to_string(path)
        .with_context(|| path.display().to_string())?
        .parse()
        .with_context(|| path.display().to_string())
}

pub fn run(path: &Path, rounds: Option<u64>, archives: bool) -> anyhow::Result<()> {
    warn_if_init();
    let sigterm = Sigterm::listen().context("écoute de SIGTERM")?;
    let cfg = load(path)?;
    let logs = cfg.logs_dir();
    fs::create_dir_all(&logs).with_context(|| logs.display().to_string())?;
    let (db, scheduler, mut standings, tournaments) = restore(&cfg, rounds)?;
    standings.set_archives(archives)?;
    let live = Live::new(cfg.data_dir().to_path_buf()).with_context(|| cfg.data_dir().display().to_string())?;

    let (reports, received) = mpsc::channel();
    let hub = live_hub(&reports);
    let remote = remote_registry(&cfg, &reports);
    let arena = Arc::new(Arena::new(cfg, Arc::clone(&hub)));
    arena.drain_on(sigterm);
    arena.sweep();
    let _api = api::serve(&arena, &hub, remote.as_ref())?;

    let pairs = Pairs::new(scheduler, Arc::clone(arena.placement()));
    let mut ladder = Ladder {
        thermal: Thermal::new(arena.config().thermal.as_ref()),
        arena,
        db,
        standings,
        tournaments,
        live,
        remote,
        pairs,

        reports,
        rounds,
        rng: rand::rng(),
        warned: HashSet::new(),
        builds: Builds::default(),

        said_waiting: false,
    };
    loop {
        if ladder.accepting() {
            if ladder.fill() {
                continue;
            }
        } else if ladder.finished() {
            break;
        }
        let Ok(report) = received.recv_timeout(IDLE_DELAY) else {
            continue;
        };
        ladder.handle(report)?;
    }
    hub.shutdown();
    Ok(())
}

/// PID 1 sans init : chaque bac à sable laisse un `containerd-shim` orphelin, rattaché à nous, que
/// personne n'attend. Les zombies s'accumulent jusqu'à la limite de processus du conteneur, puis plus
/// aucun joueur ne démarre (`no_show` partout). L'image lance un init (`docker-init`) devant nous.
fn warn_if_init() {
    if std::process::id() == 1 {
        eprintln!(
            "ATTENTION : isolation-ladder est le PID 1, sans init pour ramasser les orphelins. Les \
             zombies vont s'accumuler jusqu'à bloquer les parties : lancer le conteneur avec \
             `init: true` (compose) ou `docker run --init`."
        );
    }
}

/// Matchmaking, classement et tournoi en cours, rebâtis depuis la base.
fn restore(cfg: &Config, rounds: Option<u64>) -> anyhow::Result<(Db, Scheduler, Standings, Organizer)> {
    let db = open_db(cfg)?;
    let games = db.games().with_context(|| cfg.db.display().to_string())?;
    let scheduler = Scheduler::from_history(&games, &cfg.session);
    let standings =
        Standings::from_history(cfg.data_dir(), &games, &db).with_context(|| cfg.data_dir().display().to_string())?;
    let tournaments = Organizer::new(cfg, rounds.is_none(), &games)?;
    tournaments.publish_preview(cfg, &standings);
    Ok((db, scheduler, standings, tournaments))
}

/// État du ladder en marche, possédé par la seule boucle principale.
struct Ladder {
    arena: Arc<Arena>,
    db: Db,
    standings: Standings,
    tournaments: Organizer,
    live: Live,
    remote: Option<Arc<Registry>>,
    pairs: Pairs,
    thermal: Option<Thermal>,
    reports: Sender<Report>,
    rounds: Option<u64>,
    rng: ThreadRng,
    /// Équipes inscrites déjà signalées en doublon d'une équipe fixe.
    warned: HashSet<String>,
    builds: Builds,
    said_waiting: bool,
}

impl Ladder {
    /// Ni arrêt en cours, ni `--rounds` atteint.
    fn accepting(&self) -> bool {
        !self.arena.draining() && self.rounds.is_none_or(|n| self.pairs.round < n)
    }

    /// Plus rien ne tourne : la boucle peut s'arrêter.
    fn finished(&self) -> bool {
        self.pairs.idle() && self.arena.smoke_tests() == 0
    }

    /// Avance le tournoi et lance ce qui tient dans les slots libres. Vrai si une paire du ladder
    /// vient de partir : un autre slot est peut-être encore libre.
    fn fill(&mut self) -> bool {
        let cfg = self.arena.config();
        self.tournaments.tick(cfg, &self.standings, self.rng.random());
        self.tournaments.poll(cfg, &self.standings, self.rng.random());
        let cool = self.thermal.as_mut().is_none_or(Thermal::cool);
        if cool {
            self.pairs
                .launch_tournament(&self.arena, &mut self.tournaments, &self.reports);
        }
        let teams = roster(cfg, self.remote.as_ref(), &mut self.warned);
        let standings = &self.standings;
        let portable = portable_teams(cfg, &teams, &mut self.builds, |t, b| standings.pace(t, b));
        self.pairs.set_portable(portable, &teams);
        let waiting = self.tournaments.waiting();
        self.pairs.cancel_stale(cfg, &teams, &waiting);
        if !cool {
            return false;
        }
        self.pairs
            .launch_announced(&self.arena, &teams, &self.reports, &mut self.rng);
        if !self.pairs.open(self.rounds) {
            return false;
        }
        let unavailable = self.pairs.unavailable(&waiting, None);
        let standings = &self.standings;
        let next = self
            .pairs
            .next_pair(&teams, &unavailable, |id| standings.rating(id), &mut self.rng);
        if let Some((a, b)) = next {
            self.said_waiting = false;
            let pair = [a.clone(), b.clone()];
            return self
                .pairs
                .launch(&self.arena, &teams, pair, &self.reports, &mut self.rng);
        }
        if self.pairs.idle() && !self.said_waiting {
            eprintln!("en attente d'au moins deux équipes ({} pour l'instant)", teams.len());
            self.said_waiting = true;
        }
        false
    }

    fn handle(&mut self, report: Report) -> anyhow::Result<()> {
        let cfg = self.arena.config();
        match report {
            Report::Wake => {}
            Report::Started { red, black, game } => published(self.live.start(&red, &black, game.as_ref())),
            Report::Assigned { red, black, id } => published(self.live.assign(&red, &black, &id)),
            Report::Played {
                red,
                black,
                outcome,
                game,
            } => {
                published(self.live.end(&red, &black));
                let saved = save(cfg, &self.db, &mut self.standings, outcome, &red, &black)?;
                self.tournaments.publish_preview(cfg, &self.standings);
                if let Some(game) = game {
                    self.tournaments.record(cfg, &game, saved.as_ref());
                }
            }
            Report::Resting(rest) => {
                if !self.arena.draining() && self.pairs.open(self.rounds) {
                    let teams = roster(cfg, self.remote.as_ref(), &mut self.warned);
                    let standings = &self.standings;
                    let portable = portable_teams(cfg, &teams, &mut self.builds, |t, b| standings.pace(t, b));
                    self.pairs.set_portable(portable, &teams);
                    let waiting = self.tournaments.waiting();
                    let rating = |id: &str| standings.rating(id);
                    self.pairs.announce(cfg, &teams, &waiting, rest, rating, &mut self.rng);
                }
            }
            Report::Done { pair, lease } => {
                self.pairs.done(&pair, lease);

                prune_logs(&cfg.logs_dir(), LOG_RETENTION);
            }
        }
        Ok(())
    }
}

fn published(written: io::Result<()>) {
    if let Err(e) = written {
        eprintln!("   parties en cours non publiées : {e}");
    }
}

fn save(
    cfg: &Config,
    db: &Db,
    standings: &mut Standings,
    outcome: Result<GameRecord, GameError>,
    red: &str,
    black: &str,
) -> anyhow::Result<Option<GameRecord>> {
    let record = match outcome {
        Ok(record) => record,
        Err(e) => {
            eprintln!("   partie {red} contre {black} annulée : {e}");
            return Ok(None);
        }
    };
    db.insert_game(&record).with_context(|| cfg.db.display().to_string())?;
    if let Err(e) = standings.record(&record, db) {
        eprintln!("   classement non publié : {e}");
    }
    let result = &record.result;
    eprintln!(
        "   {red} (rouge) contre {black} (noir) : {} gagne ({:?}, {} coups)",
        result.winner,
        result.reason,
        result.moves.len()
    );
    Ok(Some(record))
}

/// Supprime les journaux plus vieux que `retention`. Un échec ne doit pas arrêter le ladder.
fn prune_logs(dir: &Path, retention: Duration) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age > retention));
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_logs_removes_only_old_files() {
        let dir = std::env::temp_dir().join(format!("isolation-logs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let (old, new) = (dir.join("old.log"), dir.join("new.log"));
        fs::write(&old, "").unwrap();
        fs::write(&new, "").unwrap();
        let expired = std::time::SystemTime::now() - LOG_RETENTION - Duration::from_hours(24);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(expired)
            .unwrap();
        prune_logs(&dir, LOG_RETENTION);
        assert!(!old.exists());
        assert!(new.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
