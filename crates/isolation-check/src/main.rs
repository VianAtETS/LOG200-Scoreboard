//! Rejoue avec notre moteur les parties d'un JSONL du ladder ou d'une sauvegarde du serveur
//! officiel (`BOARDGAME 1`).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use thiserror::Error;

use isolation_core::record::Jsonl;
use isolation_core::rules::{Replay, ReplayError};

#[derive(Parser)]
#[command(
    name = "isolation-check",
    version,
    about = "Valide des parties (JSONL du ladder ou sauvegarde BOARDGAME) ; code 1 si l'une est invalide"
)]
struct Cli {
    #[arg(required = true)]
    files: Vec<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut failures = 0;
    for path in &cli.files {
        let path_name = path.display();
        let errors = match std::fs::read_to_string(path) {
            Ok(text) if text.starts_with("BOARDGAME") => check_save(&text),
            Ok(text) => check_jsonl(&text),
            Err(e) => vec![e.to_string()],
        };
        if errors.is_empty() {
            println!("{path_name} : OK");
        }
        for e in &errors {
            println!("{path_name} : {e}");
        }
        failures += errors.len();
    }
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check_jsonl(text: &str) -> Vec<String> {
    let jsonl = Jsonl::parse(text);
    eprintln!("{} partie(s) lue(s)", jsonl.games.len());
    let skipped = jsonl
        .skipped
        .iter()
        .map(|s| format!("ligne {} ignorée : {}", s.line, s.reason));
    let invalid = jsonl
        .games
        .iter()
        .filter_map(|g| g.result.validate().err().map(|e| format!("partie {} : {e}", g.id)));
    skipped.chain(invalid).collect()
}

fn check_save(text: &str) -> Vec<String> {
    match replay_save(text) {
        Ok((count, replay)) => {
            let status = replay
                .winner
                .map_or_else(|| "en cours".into(), |w| format!("gagnant {w}"));
            eprintln!("{count} coups, {status}");
            vec![]
        }
        Err(e) => vec![e.to_string()],
    }
}

fn replay_save(text: &str) -> Result<(usize, Replay), CheckError> {
    let moves = parse_save(text)?;
    let replay = Replay::from_notation(moves.iter().copied())?;
    Ok((moves.len(), replay))
}

#[derive(Debug, Error)]
enum CheckError {
    #[error(transparent)]
    Save(#[from] SaveError),
    #[error(transparent)]
    Replay(#[from] ReplayError),
}

#[derive(Debug, Error, PartialEq, Eq)]
enum SaveError {
    #[error("plateau non géré : {0}")]
    UnsupportedBoard(String),
    #[error("en-tête moves= introuvable")]
    MissingCount,
    #[error("{declared} coups annoncés, {found} lus")]
    CountMismatch { declared: usize, found: usize },
}

/// Coups d'une sauvegarde : `BOARDGAME 1`, `rows=`, `cols=`, `start=`, `current=`, `moves=n`, puis
/// n coups.
fn parse_save(text: &str) -> Result<Vec<&str>, SaveError> {
    let mut lines = text.lines().map(str::trim);
    let mut declared = None;
    for line in lines.by_ref() {
        if let Some(n) = line.strip_prefix("moves=") {
            declared = n.parse::<usize>().ok();
            break;
        }
        let size = line.strip_prefix("rows=").or_else(|| line.strip_prefix("cols="));
        if size.is_some_and(|s| s != "12") {
            return Err(SaveError::UnsupportedBoard(line.to_string()));
        }
    }
    let declared = declared.ok_or(SaveError::MissingCount)?;
    let moves: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();
    if moves.len() != declared {
        return Err(SaveError::CountMismatch {
            declared,
            found: moves.len(),
        });
    }
    Ok(moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_header_is_checked() {
        let save = "BOARDGAME 1\nrows=12\ncols=12\nmoves=2\nG1-G4-F9\nF12-F10-G7\n";
        assert_eq!(parse_save(save), Ok(vec!["G1-G4-F9", "F12-F10-G7"]));
        assert_eq!(
            parse_save("BOARDGAME 1\nrows=8\nmoves=0\n"),
            Err(SaveError::UnsupportedBoard("rows=8".into()))
        );
        assert_eq!(parse_save("BOARDGAME 1\n"), Err(SaveError::MissingCount));
        assert_eq!(
            parse_save("BOARDGAME 1\nmoves=3\nG1-G4-F9\n"),
            Err(SaveError::CountMismatch { declared: 3, found: 1 })
        );
    }
}
