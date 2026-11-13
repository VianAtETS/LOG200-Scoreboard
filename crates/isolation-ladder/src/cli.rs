use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::ladder::{self, IDLE_DELAY};
use crate::submit;
use crate::team_id::TeamId;
use crate::tournament::admin::{self, Request};

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
    /// Afficher les filtres de versions archivées dans le classement.
    #[arg(long)]
    archives: bool,
}

#[derive(Subcommand)]
enum Command {
    AddTeam {
        id: TeamId,
        #[arg(long, default_value = submit::DEFAULT_TEAMS)]
        teams: PathBuf,
    },
    /// Requête lue par le ladder en marche.
    Tournament {
        #[command(subcommand)]
        action: TournamentAction,
    },
}

#[derive(Subcommand)]
enum TournamentAction {
    Start {
        config: PathBuf,
    },
    Resume {
        config: PathBuf,
        id: String,
        #[arg(value_name = "MATCH")]
        match_id: String,
    },
}

pub fn cli() -> ExitCode {
    let cli = Cli::parse();
    let result = match (cli.command, cli.config) {
        (Some(Command::AddTeam { id, teams }), _) => submit::add_team(&teams, id)
            .map(|token| println!("{token}"))
            .map_err(anyhow::Error::from),
        (Some(Command::Tournament { action }), _) => request(action),
        (None, Some(path)) => ladder::run(&path, cli.rounds, cli.archives),
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

fn request(action: TournamentAction) -> anyhow::Result<()> {
    let (path, request) = match action {
        TournamentAction::Start { config } => (config, Request::Start),
        TournamentAction::Resume { config, id, match_id } => (config, Request::Resume { id, match_id }),
    };
    let sent = admin::send(&ladder::load(&path)?, &request)?;
    println!(
        "requête déposée dans {} : le ladder la lit sous {} s",
        sent.display(),
        IDLE_DELAY.as_secs()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_are_opt_in() {
        assert!(
            !Cli::try_parse_from(["isolation-ladder", "ladder.toml"])
                .unwrap()
                .archives
        );
        assert!(
            Cli::try_parse_from(["isolation-ladder", "ladder.toml", "--archives"])
                .unwrap()
                .archives
        );
        assert!(
            Cli::try_parse_from(["isolation-ladder", "--archives", "ladder.toml"])
                .unwrap()
                .archives
        );
    }
}
