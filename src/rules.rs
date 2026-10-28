//! Règles du serveur officiel, détaillées dans docs/rules.md.

use std::ops::RangeInclusive;

use rand::Rng;
use rand::seq::IndexedRandom;
use thiserror::Error;

use crate::board::{Board, Color, DIRS, Move, Square};

/// Cases atteignables depuis `from` : jusqu'à 3 pas en ligne droite, sans traverser d'obstacle.
pub fn destinations(board: &Board, from: Square, mut visit: impl FnMut(Square)) {
    for (dx, dy) in DIRS {
        let mut current = from;
        for _ in 0..3 {
            match current.offset(dx, dy) {
                Some(next) if board.is_empty(next) => {
                    current = next;
                    visit(current);
                }
                _ => break,
            }
        }
    }
}

/// Nombre de cases atteignables, sans tenir compte de la contrainte de destruction.
pub fn mobility(board: &Board, color: Color) -> i32 {
    let mut count = 0;
    destinations(board, board.pawn(color), |_| count += 1);
    count
}

/// Distances au pion adverse où détruire, par ordre de repli. Le repli s'applique à l'ensemble
/// des coups : on garde le premier anneau qui en donne au moins un.
fn destroy_rings(destroyed: u32) -> &'static [RangeInclusive<i8>] {
    const EARLY: &[RangeInclusive<i8>] = &[3..=3, 2..=2, 1..=1];
    const MIDDLE: &[RangeInclusive<i8>] = &[2..=2, 1..=1];
    #[expect(clippy::single_range_in_vec_init, reason = "un seul anneau, de 1 à 2")]
    const LATE: &[RangeInclusive<i8>] = &[1..=2];
    match destroyed {
        0..10 => EARLY,
        10..20 => MIDDLE,
        _ => LATE,
    }
}

pub(crate) fn first_ring(destroyed: u32) -> RangeInclusive<i8> {
    destroy_rings(destroyed)[0].clone()
}

pub(crate) fn ring(center: Square, ring: RangeInclusive<i8>) -> impl Iterator<Item = Square> {
    DIRS.into_iter()
        .flat_map(move |(dx, dy)| ring.clone().map_while(move |k| center.offset(dx * k, dy * k)))
}

/// Cibles de l'anneau, sans exclure l'arrivée.
fn candidates(
    board: &Board,
    from: Square,
    opponent: Square,
    ring: RangeInclusive<i8>,
) -> impl Iterator<Item = Square> + '_ {
    self::ring(opponent, ring).filter(move |&target| target == from || board.is_empty(target))
}

fn push_moves(board: &Board, from: Square, opponent: Square, ring: RangeInclusive<i8>, out: &mut Vec<Move>) {
    let mut targets = [opponent; 16];
    let mut count = 0;
    for target in candidates(board, from, opponent, ring) {
        targets[count] = target;
        count += 1;
    }
    let targets = &targets[..count];
    destinations(board, from, |to| {
        out.extend(
            targets
                .iter()
                .filter(|&&target| target != to)
                .map(|&destroyed| Move { from, to, destroyed }),
        );
    });
}

/// Remplace le contenu de `out`, pour réutiliser son allocation.
pub fn generate(board: &Board, color: Color, out: &mut Vec<Move>) {
    out.clear();
    let from = board.pawn(color);
    let opponent = board.pawn(color.other());
    for ring in destroy_rings(board.destroyed()) {
        push_moves(board, from, opponent, ring.clone(), out);
        if !out.is_empty() {
            return;
        }
    }
}

pub fn legal_moves(board: &Board, color: Color) -> Vec<Move> {
    let mut moves = Vec::new();
    generate(board, color, &mut moves);
    moves
}

pub fn is_legal(board: &Board, color: Color, mv: Move) -> bool {
    mv.from == board.pawn(color) && legal_moves(board, color).contains(&mv)
}

/// Comme `!legal_moves(..).is_empty()`, sans construire la liste.
pub fn has_moves(board: &Board, color: Color) -> bool {
    playable_mobility(board, color).is_some()
}

/// La mobilité, ou `None` sans aucun coup complet.
pub fn playable_mobility(board: &Board, color: Color) -> Option<i32> {
    let from = board.pawn(color);
    let opponent = board.pawn(color.other());
    let mut first = None;
    let mut count = 0;
    destinations(board, from, |to| {
        first.get_or_insert(to);
        count += 1;
    });
    let first = first?;
    destroy_rings(board.destroyed())
        .iter()
        .any(|ring| {
            let mut targets = candidates(board, from, opponent, ring.clone());
            match (targets.next(), targets.next()) {
                (Some(_), Some(_)) => true,
                (Some(only), None) => count > 1 || only != first,
                (None, _) => false,
            }
        })
        .then_some(count)
}

/// Après un coup de `mover`, il gagne si l'adversaire n'a plus aucun coup : pas de nulle.
pub fn winner_after(board: &Board, mover: Color) -> Option<Color> {
    (!has_moves(board, mover.other())).then_some(mover)
}

/// `plies` demi-coups légaux tirés au hasard depuis le plateau initial.
pub fn random_opening(plies: usize, rng: &mut impl Rng) -> Vec<Move> {
    let mut board = Board::initial();
    let mut turn = Color::Red;
    let mut opening = Vec::with_capacity(plies);
    for _ in 0..plies {
        let Some(&mv) = legal_moves(&board, turn).choose(rng) else {
            break;
        };
        board.apply(mv);
        opening.push(mv);
        turn = turn.other();
    }
    opening
}

/// Les numéros de coup des messages commencent à 1.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("coup {} illisible : {text:?}", .index + 1)]
    Unreadable { index: usize, text: String },
    #[error("coup {} illégal : {mv}", .index + 1)]
    Illegal { index: usize, mv: Move },
    #[error("coup {} joué après la fin de la partie : {mv}", .index + 1)]
    AfterEnd { index: usize, mv: Move },
}

impl ReplayError {
    /// Position du coup fautif, à partir de 0.
    pub const fn index(&self) -> usize {
        match self {
            Self::Unreadable { index, .. } | Self::Illegal { index, .. } | Self::AfterEnd { index, .. } => *index,
        }
    }
}

/// Partie rejouée depuis le plateau initial, rouge en premier.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Replay {
    pub board: Board,
    pub turn: Color,
    /// Renseigné si le dernier coup a terminé la partie.
    pub winner: Option<Color>,
}

impl Default for Replay {
    fn default() -> Self {
        Self {
            board: Board::initial(),
            turn: Color::Red,
            winner: None,
        }
    }
}

impl Replay {
    pub fn from_moves(moves: impl IntoIterator<Item = Move>) -> Result<Self, ReplayError> {
        let mut replay = Self::default();
        for (index, mv) in moves.into_iter().enumerate() {
            replay.play(index, mv)?;
        }
        Ok(replay)
    }

    pub fn from_notation<'a>(moves: impl IntoIterator<Item = &'a str>) -> Result<Self, ReplayError> {
        let mut replay = Self::default();
        for (index, text) in moves.into_iter().enumerate() {
            let mv = text.parse().map_err(|_| ReplayError::Unreadable {
                index,
                text: text.to_string(),
            })?;
            replay.play(index, mv)?;
        }
        Ok(replay)
    }

    fn play(&mut self, index: usize, mv: Move) -> Result<(), ReplayError> {
        if self.winner.is_some() {
            return Err(ReplayError::AfterEnd { index, mv });
        }
        if !is_legal(&self.board, self.turn, mv) {
            return Err(ReplayError::Illegal { index, mv });
        }
        self.board.apply(mv);
        self.winner = winner_after(&self.board, self.turn);
        self.turn = self.turn.other();
        Ok(())
    }
}

#[cfg(test)]
pub mod test_support {
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rand::seq::IndexedRandom;

    use super::{has_moves, legal_moves};
    use crate::board::{Board, Color};

    pub fn positions(seed: u64, count: usize, max_moves: usize) -> Vec<(Board, Color)> {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut positions = Vec::new();
        while positions.len() < count {
            let mut board = Board::initial();
            let mut turn = Color::Red;
            while let Some(&mv) = legal_moves(&board, turn).choose(&mut rng) {
                board.apply(mv);
                turn = turn.other();
                let moves = legal_moves(&board, turn).len();
                if (2..=max_moves).contains(&moves) && has_moves(&board, turn.other()) {
                    positions.push((board, turn));
                    break;
                }
            }
        }
        positions
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::board::Cell;
    use crate::board::test_support::diagram;

    fn sq(s: &str) -> Square {
        s.parse().unwrap()
    }

    fn ray_distance(a: Square, b: Square) -> Option<u8> {
        let (dx, dy) = (a.x().abs_diff(b.x()), a.y().abs_diff(b.y()));
        (dx == 0 || dy == 0 || dx == dy).then_some(dx.max(dy))
    }

    #[test]
    fn initial_red_moves_destroy_at_distance_three_from_black() {
        let board = Board::initial();
        let moves = legal_moves(&board, Color::Red);
        assert!(!moves.is_empty());
        let black = board.pawn(Color::Black);
        for mv in &moves {
            assert_eq!(ray_distance(black, mv.destroyed), Some(3), "{mv}");
        }
        // F12 -> rayons à distance 3 : C12, I12, F9, C9, I9.
        let mut targets: Vec<String> = moves.iter().map(|m| m.destroyed.to_string()).collect();
        targets.sort();
        targets.dedup();
        assert_eq!(targets, ["C12", "C9", "F9", "I12", "I9"]);
    }

    #[test]
    fn opponent_pawn_blocks_movement() {
        let board = diagram(
            "............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             RB..........",
        );
        let moves = legal_moves(&board, Color::Red);
        assert!(
            moves.iter().all(|m| m.to.y() != 11),
            "déplacement horizontal à travers B"
        );
    }

    #[test]
    fn fallback_is_global_not_per_move() {
        // Noir en A12 : à distance 3, il ne reste que D9 (D12 et A9 sont détruites). Rouge peut
        // atterrir en D9 ; ce coup-là n'a plus de cible à 3, mais d'autres en ont, donc il est interdit.
        let board = diagram(
            "B..#........
             ............
             ............
             #...........
             ............
             ............
             ...R........
             ............
             ............
             ............
             ............
             ............",
        );
        let moves = legal_moves(&board, Color::Red);
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.destroyed == sq("D9")));
        assert!(moves.iter().all(|m| m.to != sq("D9")));
    }

    #[test]
    fn fallback_to_distance_two_when_no_distance_three_target() {
        let board = diagram(
            "B..#........
             ............
             ............
             #..#........
             ............
             ............
             ...R........
             ............
             ............
             ............
             ............
             ............",
        );
        let black = board.pawn(Color::Black);
        let moves = legal_moves(&board, Color::Red);
        assert!(!moves.is_empty());
        for mv in &moves {
            assert_eq!(ray_distance(black, mv.destroyed), Some(2), "{mv}");
        }
    }

    #[test]
    fn origin_square_is_destroyable_arrival_is_not() {
        // Seule cible restante à distance 3 de A12 : D9, occupée par le rouge.
        let board = diagram(
            "B..#........
             ............
             ............
             #..R........
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............",
        );
        let moves = legal_moves(&board, Color::Red);
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.destroyed == m.from));
    }

    #[test]
    fn middle_phase_uses_distance_two() {
        let board = diagram(
            "............
             ............
             ............
             ......B.....
             ............
             ............
             ............
             ............
             ............
             ...R........
             ##########..
             ............",
        );
        assert_eq!(board.destroyed(), 10);
        let black = board.pawn(Color::Black);
        for mv in legal_moves(&board, Color::Red) {
            assert_eq!(ray_distance(black, mv.destroyed), Some(2), "{mv}");
        }
    }

    #[test]
    fn final_phase_allows_distance_one_and_two() {
        let board = diagram(
            "............
             ............
             ............
             ......B.....
             ............
             ............
             ............
             ............
             ..........##
             ...R......##
             ############
             ....####....",
        );
        assert_eq!(board.destroyed(), 20);
        let black = board.pawn(Color::Black);
        let mut distances: Vec<u8> = legal_moves(&board, Color::Red)
            .iter()
            .map(|m| ray_distance(black, m.destroyed).unwrap())
            .collect();
        distances.sort_unstable();
        distances.dedup();
        assert_eq!(distances, [1, 2]);
    }

    #[test]
    fn blocked_player_has_no_moves_and_loses() {
        let board = diagram(
            "R#..........
             ##..........
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ...........B",
        );
        assert!(!has_moves(&board, Color::Red));
        assert_eq!(winner_after(&board, Color::Black), Some(Color::Black));
        assert_eq!(winner_after(&board, Color::Red), None);
    }

    #[test]
    fn mobile_player_without_target_has_no_moves() {
        // 20 cases détruites : l'anneau 1..=2 autour de A12 est entièrement détruit. Rouge peut
        // bouger, mais n'a rien à détruire.
        let board = diagram(
            "B##.........
             ##..........
             #.#.........
             ............
             ............
             ......R.....
             ############
             ##..........
             ............
             ............
             ............
             ............",
        );
        assert_eq!(board.destroyed(), 20);
        assert!(mobility(&board, Color::Red) > 0);
        assert!(!has_moves(&board, Color::Red));
    }

    #[test]
    fn single_target_on_the_only_destination_is_no_move() {
        // 20 cases détruites : autour de A12, seule B11 reste à détruire, et c'est la seule
        // destination de rouge (B10).
        let blocked = diagram(
            "B##.........
             #.#.........
             #R#.........
             ###.........
             ............
             ............
             ###########.
             ............
             ............
             ............
             ............
             ............",
        );
        assert_eq!(blocked.destroyed(), 20);
        assert!(!has_moves(&blocked, Color::Red));
        // C9 libre : rouge a d'autres destinations, et détruit B11.
        let free = diagram(
            "B##.........
             #.#.........
             #R#.........
             ##..........
             ............
             ............
             ###########.
             ............
             ............
             ............
             ............
             ............",
        );
        assert!(has_moves(&free, Color::Red));
        for board in [blocked, free] {
            assert_eq!(
                has_moves(&board, Color::Red),
                !legal_moves(&board, Color::Red).is_empty()
            );
        }
    }

    #[test]
    fn has_moves_matches_legal_moves() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..20 {
            let mut board = Board::initial();
            let mut turn = Color::Red;
            while let Some(&mv) = legal_moves(&board, turn).choose(&mut rng) {
                board.apply(mv);
                turn = turn.other();
                for color in [Color::Red, Color::Black] {
                    assert_eq!(
                        has_moves(&board, color),
                        !legal_moves(&board, color).is_empty(),
                        "\n{board}"
                    );
                }
            }
        }
    }

    #[test]
    fn replay_reports_the_first_bad_move() {
        let first = legal_moves(&Board::initial(), Color::Red)[0];
        let replay = Replay::from_moves([first]).unwrap();
        assert_eq!(replay.board.destroyed(), 1);
        assert_eq!(replay.turn, Color::Black);
        assert_eq!(replay.winner, None);
        // Le rouge rejoue : le pion à bouger n'est plus le sien.
        let err = Replay::from_moves([first, first]).unwrap_err();
        assert_eq!(err, ReplayError::Illegal { index: 1, mv: first });
        assert_eq!(err.to_string(), format!("coup 2 illégal : {first}"));
        let text = first.to_string();
        let err = Replay::from_notation([text.as_str(), "zz"]).unwrap_err();
        assert_eq!(err.index(), 1);
    }

    #[test]
    fn replay_rejects_moves_after_the_end() {
        let mut board = diagram(
            "B#..........
             #...........
             ............
             ............
             ............
             ......R.....
             ############
             ############
             ############
             ............
             ............
             ............",
        );
        let winning = legal_moves(&board, Color::Red)
            .into_iter()
            .find(|&m| {
                board.apply(m);
                let won = winner_after(&board, Color::Red).is_some();
                board.undo(m);
                won
            })
            .unwrap();
        let mut replay = Replay {
            board,
            ..Replay::default()
        };
        replay.play(0, winning).unwrap();
        assert_eq!(replay.winner, Some(Color::Red));
        assert!(matches!(
            replay.play(1, winning),
            Err(ReplayError::AfterEnd { index: 1, .. })
        ));
    }

    #[test]
    fn is_legal_checks_owner_and_membership() {
        let board = Board::initial();
        let mv = legal_moves(&board, Color::Red)[0];
        assert!(is_legal(&board, Color::Red, mv));
        assert!(!is_legal(&board, Color::Black, mv));
        let bad = Move { destroyed: mv.to, ..mv };
        assert!(!is_legal(&board, Color::Red, bad));
    }

    #[test]
    fn random_opening_is_legal() {
        let opening = random_opening(4, &mut StdRng::seed_from_u64(7));
        assert_eq!(opening.len(), 4);
        let replay = Replay::from_moves(opening).unwrap();
        assert_eq!(replay.turn, Color::Red);
        assert_eq!(
            replay.board.cells().iter().filter(|&&c| c == Cell::Destroyed).count(),
            4
        );
    }
}
