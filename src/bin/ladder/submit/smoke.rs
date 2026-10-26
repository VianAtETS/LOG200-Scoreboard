//! Smoke test d'un jar : une partie par couleur contre le bot aléatoire, dans le bac à sable.

use std::fmt;
use std::path::Path;

use isolation::board::Color;
use isolation::record::{GameResult, Reason};
use isolation::rules::random_opening;

use super::auth::TeamId;
use crate::arena::{Arena, GameError};
use crate::config::{SubmitConfig, Team};

/// Perdre n'est pas un échec ; perdre par forfait (temps, connexion, absence, coups invalides) l'est.
#[derive(Debug, PartialEq, Eq)]
pub struct Forfeit {
    side: Color,
    why: &'static str,
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
        Some(Self { side, why })
    }
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
    let candidate = cfg.team_with_jar(id, jar);
    let opponent = Team {
        id: SMOKE_OPPONENT.into(),
        cmd: submit.opponent.clone(),
        build: None,
        anchor: false,
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
                    return Verdict::Rejected(forfeit);
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
}
