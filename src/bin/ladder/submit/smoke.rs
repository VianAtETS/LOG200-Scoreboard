//! Smoke test d'un jar : une partie par couleur contre le bot aléatoire, dans le bac à sable.

use std::fmt;
use std::fs;
use std::path::Path;

use isolation::board::Color;
use isolation::record::{GameResult, Reason};
use isolation::rules::random_opening;

use super::auth::TeamId;
use crate::arena::{Arena, GameError};
use crate::config::{SubmitConfig, Team};

/// Fin de la sortie d'erreur renvoyée à l'équipe : de quoi voir l'exception, pas tout le journal.
const STDERR_TAIL_LINES: usize = 20;
const STDERR_TAIL_BYTES: usize = 4096;

/// Perdre n'est pas un échec ; perdre par forfait (temps, connexion, absence, coups invalides) l'est.
#[derive(Debug, PartialEq, Eq)]
pub struct Forfeit {
    side: Color,
    why: &'static str,
    stderr: String,
}

impl Forfeit {
    fn of(result: &GameResult, side: Color) -> Option<Self> {
        if result.winner == side {
            return None;
        }
        let why = match result.reason {
            Reason::NoMoves => return None,
            Reason::Timeout => "temps dépassé",
            Reason::Disconnect => "connexion perdue",
            Reason::NoShow => "jamais connecté à l'arbitre",
            Reason::Protocol => "trop de coups invalides",
        };
        Some(Self {
            side,
            why,
            stderr: String::new(),
        })
    }

    /// Un jar qui plante au lancement (version de Java, manifeste...) l'explique sur sa sortie d'erreur.
    fn with_stderr_of(self, log: &Path) -> Self {
        Self {
            stderr: stderr_tail(log),
            ..self
        }
    }

    /// Bloc à ajouter au message de refus, vide si le jar n'a rien écrit.
    pub fn stderr_section(&self) -> String {
        if self.stderr.is_empty() {
            String::new()
        } else {
            format!("\n\nFin de la sortie d'erreur du jar :\n{}", self.stderr)
        }
    }
}

fn stderr_tail(log: &Path) -> String {
    let Ok(bytes) = fs::read(log) else {
        return String::new();
    };
    let start = bytes.len().saturating_sub(STDERR_TAIL_BYTES);
    let text = String::from_utf8_lossy(&bytes[start..]);
    let lines: Vec<&str> = text.trim_end().lines().collect();
    lines[lines.len().saturating_sub(STDERR_TAIL_LINES)..].join("\n")
}

impl fmt::Display for Forfeit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let side = match self.side {
            Color::Red => "rouge",
            Color::Black => "noir",
        };
        write!(f, "en {side} : {}", self.why)
    }
}

pub enum Verdict {
    Accepted,
    Rejected(Forfeit),
    Error(GameError),
}

pub const SMOKE_OPPONENT: &str = "test-aleatoire";

pub fn smoke_test(arena: &Arena, submit: &SubmitConfig, id: &TeamId, jar: &Path) -> Verdict {
    let cfg = arena.config();
    let candidate = cfg.team_with_jar(id, jar, false);
    let opponent = Team {
        id: SMOKE_OPPONENT.into(),
        cmd: submit.opponent.clone(),
        build: None,
        anchor: false,
        multithread: false,
    };
    for (red, black, side) in [
        (&candidate, &opponent, Color::Red),
        (&opponent, &candidate, Color::Black),
    ] {
        let opening = random_opening(cfg.opening_plies, &mut rand::rng());
        match arena.play(red, black, &opening) {
            Err(e) => return Verdict::Error(e),
            Ok(record) => {
                if let Some(forfeit) = Forfeit::of(&record.result, side) {
                    let log = cfg.player_log(&record.id, &candidate.id);
                    return Verdict::Rejected(forfeit.with_stderr_of(&log));
                }
            }
        }
    }
    Verdict::Accepted
}

#[cfg(test)]
mod tests {
    use super::*;
    use isolation::record::{InvalidCounts, RECORD_VERSION};

    fn result(winner: Color, reason: Reason) -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            time_ms: 500,
            opening: vec![],
            moves: vec![],
            winner,
            reason,
            invalid: InvalidCounts::default(),
            duration_ms: 0,
            move_ms: vec![],
        }
    }

    #[test]
    fn only_forfeits_fail() {
        assert_eq!(Forfeit::of(&result(Color::Red, Reason::Timeout), Color::Red), None);
        assert_eq!(Forfeit::of(&result(Color::Black, Reason::NoMoves), Color::Red), None);
        for reason in [Reason::Timeout, Reason::Disconnect, Reason::NoShow, Reason::Protocol] {
            assert!(Forfeit::of(&result(Color::Black, reason), Color::Red).is_some());
        }
        let forfeit = Forfeit::of(&result(Color::Red, Reason::Timeout), Color::Black).unwrap();
        assert_eq!(forfeit.to_string(), "en noir : temps dépassé");
    }

    #[test]
    fn rejection_ends_with_the_jar_stderr() {
        let dir = crate::submit::test_support::temp_dir("smoke-stderr");
        let no_show = || Forfeit::of(&result(Color::Black, Reason::NoShow), Color::Red).unwrap();
        assert_eq!(no_show().with_stderr_of(&dir.join("absent.log")).stderr_section(), "");

        let log = dir.join("jar.log");
        let mut text: Vec<String> = (0..100).map(|i| format!("ligne {i}")).collect();
        text.push("java.lang.UnsupportedClassVersionError: Client\n\n".into());
        fs::write(&log, text.join("\n")).unwrap();
        let section = no_show().with_stderr_of(&log).stderr_section();
        assert!(
            section.starts_with("\n\nFin de la sortie d'erreur du jar :\nligne 81\n"),
            "{section}"
        );
        assert!(section.ends_with("ligne 99\njava.lang.UnsupportedClassVersionError: Client"));
    }
}
