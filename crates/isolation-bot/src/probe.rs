//! Coups que notre moteur refuse : si le serveur en accepte un, nos règles diffèrent des siennes.

use std::fmt;

use isolation::board::{Board, Color, DIRS, Move, Square};
use isolation::rules::{destinations, legal_moves};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProbeKind {
    /// Détruire sa propre case d'arrivée.
    Arrival,
    /// Détruire à une distance du pion adverse que la phase, ou le repli, n'autorise pas.
    Distance,
    /// Passer par-dessus le pion adverse.
    Jump,
}

impl ProbeKind {
    const ALL: [Self; 3] = [Self::Arrival, Self::Distance, Self::Jump];

    fn candidates(self, board: &Board, color: Color, legal: &[Move]) -> Vec<Move> {
        let from = board.pawn(color);
        let opponent = board.pawn(color.other());
        match self {
            Self::Arrival => arrival(board, from),
            Self::Distance => distance(board, from, opponent),
            Self::Jump => legal
                .first()
                .map_or_else(Vec::new, |m| jump(board, from, opponent, m.destroyed)),
        }
    }
}

impl fmt::Display for ProbeKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Self::Arrival => "arrival",
            Self::Distance => "distance",
            Self::Jump => "jump",
        })
    }
}

/// Une sonde pour ce tour ; la catégorie et le coup tournent d'un tour à l'autre.
pub fn pick(board: &Board, color: Color, turn: usize) -> Option<(Move, ProbeKind)> {
    let legal = legal_moves(board, color);
    (0..ProbeKind::ALL.len())
        .map(|i| ProbeKind::ALL[(turn + i) % ProbeKind::ALL.len()])
        .find_map(|kind| {
            let mut probes = kind.candidates(board, color, &legal);
            probes.retain(|m| !legal.contains(m));
            (!probes.is_empty()).then(|| (probes[turn % probes.len()], kind))
        })
}

fn arrival(board: &Board, from: Square) -> Vec<Move> {
    let mut probes = Vec::new();
    destinations(board, from, |to| {
        probes.push(Move {
            from,
            to,
            destroyed: to,
        });
    });
    probes
}

fn distance(board: &Board, from: Square, opponent: Square) -> Vec<Move> {
    let mut probes = Vec::new();
    destinations(board, from, |to| {
        for (dx, dy) in DIRS {
            for k in 1..=3 {
                if let Some(target) = opponent.offset(dx * k, dy * k)
                    && target != to
                    && (target == from || board.is_empty(target))
                {
                    probes.push(Move {
                        from,
                        to,
                        destroyed: target,
                    });
                }
            }
        }
    });
    probes
}

/// Arrivées derrière le pion adverse, en détruisant une cible légale par ailleurs.
fn jump(board: &Board, from: Square, opponent: Square, target: Square) -> Vec<Move> {
    let mut probes = Vec::new();
    for (dx, dy) in DIRS {
        let mut current = from;
        let mut jumped = false;
        for _ in 0..3 {
            let Some(next) = current.offset(dx, dy) else {
                break;
            };
            current = next;
            if next == opponent {
                jumped = true;
            } else if !board.is_empty(next) {
                break;
            } else if jumped && next != target {
                probes.push(Move {
                    from,
                    to: next,
                    destroyed: target,
                });
            }
        }
    }
    probes
}
