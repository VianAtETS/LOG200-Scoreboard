//! Enchaîne les parties entre équipes, une paire par slot libre, et ajoute une ligne par partie au
//! JSONL. Les équipes sont celles de `ladder.toml` plus les équipes inscrites qui ont un jar
//! accepté ou une session distante ouverte ; la liste est relue avant chaque paire.

mod arena;
mod config;
mod journal;
mod live;
mod remote;
mod schedule;
mod standings;
mod submit;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};

use isolation::board::Move;
use isolation::notation;
use isolation::record::{GameRecord, Jsonl};
use isolation::rules::random_opening;

use arena::{Arena, GameError};
use config::{Config, Team};
use journal::append_jsonl;
use live::Live;
use remote::Registry;
use schedule::Scheduler;
use standings::Standings;
use submit::TeamId;

/// Pause après une partie annulée : une config cassée ne doit pas faire tourner le ladder à vide.
const RETRY_DELAY: Duration = Duration::from_secs(5);
const IDLE_DELAY: Duration = Duration::from_secs(10);
const LOG_RETENTION: Duration = Duration::from_hours(90 * 24);

#[derive(Parser)]
#[command(
    name = "isolation-ladder",
    version,
    about = "Ladder Isolation : parties en continu et soumission des jars"
)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(required = true)]
    config: Option<PathBuf>,
    #[arg(long)]
    rounds: Option<u64>,
}

#[derive(Subcommand)]
enum Command {
    AddTeam {
        id: TeamId,
        #[arg(long, default_value = submit::DEFAULT_TEAMS)]
        teams: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match (cli.command, cli.config) {
        (Some(Command::AddTeam { id, teams }), _) => submit::add_team(&teams, id)
            .map(|token| println!("{token}"))
            .map_err(anyhow::Error::from),
        (None, Some(path)) => run(&path, cli.rounds),
        (None, None) => unreachable!("clap exige la configuration sans sous-commande"),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(path: &PathBuf, rounds: Option<u64>) -> anyhow::Result<()> {
    let cfg: Config = fs::read_to_string(path)
        .with_context(|| path.display().to_string())?
        .parse()
        .with_context(|| path.display().to_string())?;
    let logs = cfg.logs_dir();
    fs::create_dir_all(&logs).with_context(|| logs.display().to_string())?;
    let history = fs::read_to_string(&cfg.out)
        .map(|text| Jsonl::parse(&text))
        .unwrap_or_default();
    let mut scheduler = Scheduler::from_history(&history.games, &cfg.session);
    let mut standings = Standings::from_history(cfg.data_dir(), &history.games, &history.at)
        .with_context(|| cfg.data_dir().display().to_string())?;
    drop(history);
    let mut live = Live::new(cfg.data_dir().to_path_buf()).with_context(|| cfg.data_dir().display().to_string())?;

    let arena = Arc::new(Arena::new(cfg));
    arena.sweep();
    submit::serve(Arc::clone(&arena))?;
    let cfg = arena.config();
    let (reports, received) = mpsc::channel();
    let remote = serve_remote(cfg, &reports)?;

    let mut rng = rand::rng();
    let mut round = 0;
    let mut waiting = false;
    let mut warned = HashSet::new();
    // Paires en cours, par genre de slot (ordinaire, multithread).
    let mut running = [0; 2];
    let mut busy = HashSet::new();
    loop {
        if rounds.is_none_or(|n| round < n) {
            if running.iter().sum::<usize>() < cfg.slots() {
                let teams = roster(cfg, remote.as_ref(), &mut warned);
                let fits = |a: &Team, b: &Team| {
                    let multithread = cfg.multithread_game(a, b);
                    running[usize::from(multithread)] < cfg.slots_of(multithread)
                };
                if let Some((a, b)) = scheduler.next_pair(&teams, &busy, fits, |id| standings.rating(id), &mut rng) {
                    waiting = false;
                    round += 1;
                    running[usize::from(cfg.multithread_game(a, b))] += 1;
                    scheduler.record(&a.id, &b.id, 2);
                    busy.extend([a.id.clone(), b.id.clone()]);
                    let opening = random_opening(cfg.opening_plies, &mut rng);
                    eprintln!(
                        "== paire {round} : {} contre {} (ouverture {})",
                        a.id,
                        b.id,
                        notation::join(&opening)
                    );
                    spawn_pair(Arc::clone(&arena), a.clone(), b.clone(), opening, reports.clone());
                    continue;
                }
                if running == [0; 2] && !waiting {
                    eprintln!("en attente d'au moins deux équipes ({} pour l'instant)", teams.len());
                    waiting = true;
                }
            }
        } else if running == [0; 2] {
            break;
        }
        let Ok(report) = received.recv_timeout(IDLE_DELAY) else {
            continue;
        };
        match report {
            Report::Wake => {}
            Report::Started { red, black } => {
                if let Err(e) = live.start(&red, &black) {
                    eprintln!("   parties en cours non publiées : {e}");
                }
            }
            Report::Played { red, black, outcome } => {
                if let Err(e) = live.end(&red, &black) {
                    eprintln!("   parties en cours non publiées : {e}");
                }
                match outcome {
                    Ok(record) => {
                        let at = append_jsonl(&cfg.out, &record).with_context(|| cfg.out.display().to_string())?;
                        if let Err(e) = standings.record(&record, at) {
                            eprintln!("   classement non publié : {e}");
                        }
                        let result = &record.result;
                        eprintln!(
                            "   {red} (rouge) contre {black} (noir) : {} gagne ({:?}, {} coups)",
                            result.winner,
                            result.reason,
                            result.moves.len()
                        );
                    }
                    Err(e) => eprintln!("   partie {red} contre {black} annulée : {e}"),
                }
            }
            Report::Done { pair, multithread } => {
                running[usize::from(multithread)] -= 1;
                for id in &pair {
                    busy.remove(id);
                }
                prune_logs(&logs, LOG_RETENTION);
            }
        }
    }
    Ok(())
}

/// Compte rendu d'un thread de paire, ou réveil à l'arrivée d'une équipe distante.
#[expect(clippy::large_enum_variant, reason = "cinq messages par paire")]
enum Report {
    Wake,
    Started {
        red: String,
        black: String,
    },
    Played {
        red: String,
        black: String,
        outcome: Result<GameRecord, GameError>,
    },
    Done {
        pair: [String; 2],
        multithread: bool,
    },
}

/// Registre des équipes distantes, si `[remote]` est configuré : il réveille la boucle à chaque arrivée.
fn serve_remote(cfg: &Config, reports: &Sender<Report>) -> anyhow::Result<Option<Arc<Registry>>> {
    if cfg.remote.is_none() {
        return Ok(None);
    }
    let wake = reports.clone();
    let interval = cfg.submit.as_ref().map_or(0, |submit| submit.min_interval_s);
    let registry = Registry::new(Duration::from_secs(interval), move || {
        let _ = wake.send(Report::Wake);
    });
    remote::serve(&registry, cfg)?;
    Ok(Some(registry))
}

/// Joue la paire dans les deux couleurs, avec la même ouverture. L'arène attribue un slot à chaque partie.
fn spawn_pair(arena: Arc<Arena>, a: Team, b: Team, opening: Vec<Move>, reports: Sender<Report>) {
    thread::spawn(move || {
        for (red, black) in [(&a, &b), (&b, &a)] {
            let _ = reports.send(Report::Started {
                red: red.id.clone(),
                black: black.id.clone(),
            });
            let outcome = arena.play(red, black, &opening);
            let failed = outcome.is_err();
            let _ = reports.send(Report::Played {
                red: red.id.clone(),
                black: black.id.clone(),
                outcome,
            });
            if failed {
                thread::sleep(RETRY_DELAY);
            }
        }
        thread::sleep(Duration::from_secs(arena.config().pause_s));
        let multithread = arena.config().multithread_game(&a, &b);
        let _ = reports.send(Report::Done {
            pair: [a.id, b.id],
            multithread,
        });
    });
}

/// Équipes de `[[team]]`, puis les équipes inscrites qui n'y figurent pas. Une équipe connectée en
/// distant joue à la place de son jar.
fn roster(cfg: &Config, remote: Option<&Arc<Registry>>, warned: &mut HashSet<String>) -> Vec<Team> {
    let mut teams = cfg.teams.clone();
    let mut registered = submit::active_teams(cfg).unwrap_or_else(|e| {
        eprintln!("équipes inscrites ignorées : {e}");
        Vec::new()
    });
    for online in remote.map(Registry::online).unwrap_or_default() {
        registered.retain(|team| team.id != online.id);
        registered.push(online);
    }
    for team in registered {
        let fixed = teams.iter().any(|t| t.id == team.id);
        if (fixed || team.id == submit::SMOKE_OPPONENT) && warned.insert(team.id.clone()) {
            if fixed {
                eprintln!(
                    "équipe inscrite {} ignorée : une équipe fixe de la config a le même id",
                    team.id
                );
            } else {
                eprintln!("équipe inscrite {} : même id que l'adversaire du smoke test", team.id);
            }
        }
        if !fixed {
            teams.push(team);
        }
    }
    teams
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

    #[test]
    fn registered_team_named_like_a_fixed_team_is_reported_once() {
        let dir = std::env::temp_dir().join(format!("isolation-roster-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n\
             [submit]\nteams = {teams:?}\njars = {jars:?}\n\
             [[team]]\nid = \"niveau-1\"\ncmd = [\"bot\"]\n",
            teams = dir.join("teams.toml"),
            jars = dir.join("jars"),
        );
        let cfg: Config = toml.parse().unwrap();
        let submit = cfg.submit.clone().unwrap();
        for id in ["niveau-1", "eq01", submit::SMOKE_OPPONENT] {
            let id: TeamId = id.parse().unwrap();
            submit::add_team(&submit.teams, id.clone()).unwrap();
            let jar = submit.current_jar(&id);
            fs::create_dir_all(jar.parent().unwrap()).unwrap();
            fs::write(jar, "").unwrap();
        }
        let mut warned = HashSet::new();
        for _ in 0..2 {
            let ids: Vec<String> = roster(&cfg, None, &mut warned).into_iter().map(|t| t.id).collect();
            assert_eq!(ids, ["niveau-1", "eq01", submit::SMOKE_OPPONENT]);
        }
        assert_eq!(
            warned,
            HashSet::from(["niveau-1".into(), submit::SMOKE_OPPONENT.into()])
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
