//! Enchaîne les parties entre équipes et ajoute une ligne par partie au JSONL. Les équipes sont
//! celles de `ladder.toml` plus les équipes inscrites qui ont un jar accepté ; la liste est relue
//! avant chaque paire.

mod arena;
mod config;
mod digest;
mod journal;
mod schedule;
mod standings;
mod submit;

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};

use isolation::notation;
use isolation::record::Jsonl;
use isolation::rules::random_opening;

use arena::Arena;
use config::{Config, Team};
use journal::append_jsonl;
use schedule::Scheduler;
use standings::Standings;
use submit::TeamId;

/// Pause après une partie annulée : une config cassée ne doit pas faire tourner le ladder à vide.
const RETRY_DELAY: Duration = Duration::from_secs(5);
const IDLE_DELAY: Duration = Duration::from_secs(10);

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
    /// Configuration (voir ladder.example.toml).
    #[arg(required = true)]
    config: Option<PathBuf>,
    /// Nombre de paires à jouer ; sans fin par défaut.
    #[arg(long)]
    rounds: Option<u64>,
}

#[derive(Subcommand)]
enum Command {
    /// Inscrit une équipe et affiche son token, une seule fois.
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

    let arena = Arc::new(Arena::new(cfg));
    arena.sweep();
    submit::serve(Arc::clone(&arena))?;
    let cfg = arena.config();

    let mut rng = rand::rng();
    let mut round = 0;
    let mut waiting = false;
    while rounds.is_none_or(|n| round < n) {
        let teams = roster(cfg);
        let Some((a, b)) = scheduler.next_pair(&teams, &mut rng) else {
            if !waiting {
                eprintln!("en attente d'au moins deux équipes ({} pour l'instant)", teams.len());
                waiting = true;
            }
            thread::sleep(IDLE_DELAY);
            continue;
        };
        waiting = false;
        round += 1;
        let opening = random_opening(cfg.opening_plies, &mut rng);
        eprintln!(
            "== paire {round} : {} contre {} (ouverture {})",
            a.id,
            b.id,
            notation::join(&opening)
        );
        for (red, black) in [(a, b), (b, a)] {
            match arena.play(red, black, &opening) {
                Ok(record) => {
                    let at = append_jsonl(&cfg.out, &record).with_context(|| cfg.out.display().to_string())?;
                    if let Err(e) = standings.record(&record, at) {
                        eprintln!("   classement non publié : {e}");
                    }
                    let result = &record.result;
                    eprintln!(
                        "   {} (rouge) contre {} (noir) : {} gagne ({:?}, {} coups)",
                        red.id,
                        black.id,
                        result.winner,
                        result.reason,
                        result.moves.len()
                    );
                }
                Err(e) => {
                    eprintln!("   partie {} contre {} annulée : {e}", red.id, black.id);
                    thread::sleep(RETRY_DELAY);
                }
            }
        }
        scheduler.record(&a.id, &b.id, 2);
        thread::sleep(Duration::from_secs(cfg.pause_s));
    }
    Ok(())
}

/// Équipes de `[[team]]`, puis les équipes inscrites qui n'y figurent pas.
fn roster(cfg: &Config) -> Vec<Team> {
    let mut teams = cfg.teams.clone();
    let registered = submit::active_teams(cfg).unwrap_or_else(|e| {
        eprintln!("équipes inscrites ignorées : {e}");
        Vec::new()
    });
    for team in registered {
        if !teams.iter().any(|t| t.id == team.id) {
            teams.push(team);
        }
    }
    teams
}
