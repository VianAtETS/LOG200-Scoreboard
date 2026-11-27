//! Isolation comme `game_kit::Game` : ce que l'arbitre générique, le rejeu et la validation des
//! résultats utilisent. Les règles elles-mêmes sont dans `rules`.

use std::collections::BTreeMap;

use game_kit::descriptor::{Descriptor, Piece, SideInfo};
use game_kit::record::Reason;
use game_kit::{ByColor, Change, Color, Ending, Game, Role};
use rand::Rng;

use crate::board::{Board, Cell, Move, SIZE};
use crate::rules;

/// Version des règles et de l'affichage, celle du descripteur. À monter à chaque changement de
/// règle : les parties déjà jouées gardent la leur.
pub const VERSION: u32 = 1;

#[derive(Clone, Copy, Default, Debug)]
pub struct Isolation;

impl Game for Isolation {
    type State = Board;
    type Move = Move;

    fn descriptor(&self) -> Descriptor {
        let piece = |cell: Cell, label: &str, style: &str| Piece {
            code: cell.code(),
            label: label.into(),
            style: style.into(),
            side: cell.color(),
        };
        Descriptor {
            game: "isolation".into(),
            version: VERSION,
            name: "Isolation".into(),
            rows: SIZE,
            cols: SIZE,
            sides: ByColor {
                red: SideInfo { label: "Rouge".into() },
                black: SideInfo { label: "Noir".into() },
            },
            initial: self.cells(&Board::initial()),
            pieces: vec![
                piece(Cell::Empty, "case vide", "empty"),
                piece(Cell::Destroyed, "case détruite", "hole"),
                piece(Cell::Black, "pion noir", "pawn"),
                piece(Cell::Red, "pion rouge", "pawn"),
            ],
            special: Vec::new(),
            reasons: BTreeMap::from([("no_moves".into(), "plus aucun coup".into())]),
            max_plies: None,
        }
    }

    fn initial(&self) -> Board {
        Board::initial()
    }

    fn cells(&self, board: &Board) -> Vec<u8> {
        board.cells().iter().map(|cell| cell.code()).collect()
    }

    fn is_legal(&self, board: &Board, side: Color, mv: Move) -> bool {
        rules::is_legal(board, side, mv)
    }

    fn legal_moves(&self, board: &Board, side: Color) -> Vec<Move> {
        rules::legal_moves(board, side)
    }

    fn apply(&self, board: &mut Board, side: Color, mv: Move) -> Vec<Change> {
        board.apply(mv);
        // La case de départ peut être celle qui est détruite : les effets s'appliquent dans l'ordre.
        vec![
            Change::new(mv.from.index(), Cell::Empty.code(), Role::From),
            Change::new(mv.to.index(), Cell::pawn(side).code(), Role::To),
            Change::new(mv.destroyed.index(), Cell::Destroyed.code(), Role::Effect),
        ]
    }

    fn ending(&self, board: &Board, mover: Color) -> Option<Ending> {
        rules::winner_after(board, mover).map(|winner| Ending {
            winner: Some(winner),
            reason: Reason::NoMoves,
        })
    }

    fn protocol_move(&self, mv: Move) -> String {
        mv.protocol().to_string()
    }

    /// Celle de `rules`, pour garder les ouvertures déjà tirées avec une graine donnée.
    fn random_opening<R: Rng>(&self, plies: usize, rng: &mut R) -> Vec<Move> {
        rules::random_opening(plies, rng)
    }
}

#[cfg(test)]
mod tests {
    use game_kit::record::{GameResult, InvalidCounts, RECORD_VERSION, ValidationError};
    use game_kit::replay::Replay;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rand::seq::IndexedRandom;

    use super::*;

    fn result(moves: Vec<String>, winner: Color, reason: Reason) -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            game: None,
            game_version: None,
            time_ms: 500,
            opening: vec![],
            moves,
            winner: Some(winner),
            reason,
            invalid: InvalidCounts::default(),
            duration_ms: 1234,
            move_ms: vec![],
        }
    }

    fn first_move() -> Vec<String> {
        vec![rules::legal_moves(&Board::initial(), Color::Red)[0].to_string()]
    }

    /// Une partie complète au hasard, jusqu'à ce qu'un joueur n'ait plus de coup.
    fn random_game(seed: u64) -> (Vec<Move>, Ending) {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut replay = Replay::new(&Isolation);
        let mut moves = Vec::new();
        loop {
            let mv = *Isolation
                .legal_moves(&replay.state, replay.turn)
                .choose(&mut rng)
                .unwrap();
            replay.play(&Isolation, mv).unwrap();
            moves.push(mv);
            if let Some(ending) = replay.ending.clone() {
                return (moves, ending);
            }
        }
    }

    #[test]
    fn the_descriptor_is_consistent() {
        let descriptor = Isolation.descriptor();
        assert_eq!(descriptor.problems(), Vec::<String>::new());
        assert_eq!(descriptor.initial[5], Cell::Black.code(), "F12");
        assert_eq!(descriptor.initial[138], Cell::Red.code(), "G1");
    }

    #[test]
    fn changes_rebuild_every_position_from_the_initial_cells() {
        for seed in 0..20 {
            let (moves, _) = random_game(seed);
            let replay = Replay::from_moves(&Isolation, moves).unwrap();
            let mut cells = Isolation.descriptor().initial;
            for change in replay.changes.iter().flatten() {
                cells[usize::from(change.cell)] = change.code;
            }
            assert_eq!(cells, Isolation.cells(&replay.state), "graine {seed}");
        }
    }

    #[test]
    fn a_finished_game_validates_only_with_its_winner() {
        let (moves, ending) = random_game(3);
        let winner = ending.winner.unwrap();
        let moves: Vec<String> = moves.iter().map(ToString::to_string).collect();
        assert_eq!(
            result(moves.clone(), winner, Reason::NoMoves).validate(&Isolation),
            Ok(())
        );
        assert_eq!(
            result(moves.clone(), winner.other(), Reason::NoMoves).validate(&Isolation),
            Err(ValidationError::WrongEnding { replayed: Some(ending) })
        );
        assert_eq!(
            result(moves, winner, Reason::Timeout).validate(&Isolation),
            Err(ValidationError::ForfeitAfterEnd)
        );
    }

    #[test]
    fn validate_checks_winner_consistency() {
        // Après un coup rouge, c'est au noir : un timeout le fait perdre.
        assert_eq!(
            result(first_move(), Color::Red, Reason::Timeout).validate(&Isolation),
            Ok(())
        );
        assert_eq!(
            result(first_move(), Color::Black, Reason::Timeout).validate(&Isolation),
            Err(ValidationError::WinnerToMove(Color::Black))
        );
        assert_eq!(
            result(first_move(), Color::Red, Reason::NoMoves).validate(&Isolation),
            Err(ValidationError::WrongEnding { replayed: None })
        );
    }

    #[test]
    fn no_show_requires_no_moves_and_accepts_either_winner() {
        let absent = result(first_move(), Color::Red, Reason::NoShow);
        assert_eq!(absent.validate(&Isolation), Err(ValidationError::NoShowAfterMoves));
        for winner in [Color::Red, Color::Black] {
            assert_eq!(
                result(vec![], winner, Reason::NoShow).validate(&Isolation),
                Ok(()),
                "{winner:?}"
            );
        }
        let v: serde_json::Value = serde_json::to_value(Reason::NoShow).unwrap();
        assert_eq!(v, "no_show");
    }

    #[test]
    fn an_illegal_move_is_reported_with_its_number() {
        let mut moves = first_move();
        moves.push(moves[0].clone());
        let error = result(moves, Color::Red, Reason::Timeout)
            .validate(&Isolation)
            .unwrap_err();
        assert!(error.to_string().starts_with("coup 2 illégal"), "{error}");
    }

    #[test]
    fn the_default_opening_matches_the_historical_one() {
        // Même tirage que `rules::random_opening`, que la surcharge garde.
        let ours = Isolation.random_opening(2, &mut StdRng::seed_from_u64(42));
        let default = DefaultOpening.random_opening(2, &mut StdRng::seed_from_u64(42));
        assert_eq!(ours, rules::random_opening(2, &mut StdRng::seed_from_u64(42)));
        assert_eq!(
            ours, default,
            "le tirage par défaut de game-kit tombe sur les mêmes coups"
        );
    }

    /// Isolation sans sa surcharge de `random_opening`.
    struct DefaultOpening;

    impl Game for DefaultOpening {
        type State = Board;
        type Move = Move;

        fn descriptor(&self) -> Descriptor {
            Isolation.descriptor()
        }
        fn initial(&self) -> Board {
            Isolation.initial()
        }
        fn cells(&self, board: &Board) -> Vec<u8> {
            Isolation.cells(board)
        }
        fn is_legal(&self, board: &Board, side: Color, mv: Move) -> bool {
            Isolation.is_legal(board, side, mv)
        }
        fn legal_moves(&self, board: &Board, side: Color) -> Vec<Move> {
            Isolation.legal_moves(board, side)
        }
        fn apply(&self, board: &mut Board, side: Color, mv: Move) -> Vec<Change> {
            Isolation.apply(board, side, mv)
        }
        fn ending(&self, board: &Board, mover: Color) -> Option<Ending> {
            Isolation.ending(board, mover)
        }
    }
}
