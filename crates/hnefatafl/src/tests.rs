//! Les figures de l'énoncé, et les cas limites relevés dans le code du serveur officiel.

use game_kit::record::{GameResult, InvalidCounts, RECORD_VERSION};
use game_kit::replay::Replay;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;

use super::*;

fn at(name: &str) -> u8 {
    square(name.as_bytes()).filter(|(_, rest)| rest.is_empty()).unwrap().0
}

fn mv(text: &str) -> Move {
    text.parse().unwrap()
}

/// Un plateau vide avec ces pièces ; le roi doit en être.
fn board(pieces: &[(&str, u8)]) -> Board {
    let mut cells = [EMPTY; CELLS];
    for &(name, code) in pieces {
        cells[usize::from(at(name))] = code;
    }
    Board::from_cells(&cells).unwrap()
}

fn played(mut b: Board, text: &str) -> (Board, Vec<u8>) {
    let m = mv(text);
    let side = side_of(b.get(m.from)).unwrap();
    assert!(Hnefatafl.is_legal(&b, side, m), "{text} illégal");
    let captured = b.play(m);
    (b, captured)
}

#[test]
fn the_start_is_the_assignments_figure() {
    let b = Board::initial();
    let count = |code: u8| b.cells().iter().map(|&c| usize::from(c == code)).sum::<usize>();
    assert_eq!((count(ATTACKER), count(DEFENDER), count(KING)), (24, 12, 1));
    assert_eq!(b.king(), THRONE);
    assert_eq!(cell_name(THRONE), "G7");
    for name in ["E13", "I13", "G12", "A5", "A9", "B7", "L7", "M7", "G2", "E1"] {
        assert_eq!(b.get(at(name)), ATTACKER, "{name}");
    }
    for name in ["G10", "G4", "D7", "J7", "F7"] {
        assert_eq!(b.get(at(name)), DEFENDER, "{name}");
    }
    assert_eq!(Hnefatafl.descriptor().problems(), Vec::<String>::new());
}

#[test]
fn a_piece_is_taken_between_two_enemies_one_or_several_at_a_time() {
    // Figure 1 : horizontalement, puis verticalement.
    let (b, taken) = played(
        board(&[("B7", ATTACKER), ("C7", DEFENDER), ("E7", ATTACKER), ("G1", KING)]),
        "E7-D7",
    );
    assert_eq!((taken, b.get(at("C7"))), (vec![at("C7")], EMPTY));
    let (_, taken) = played(
        board(&[("C9", ATTACKER), ("C8", DEFENDER), ("E7", ATTACKER), ("G1", KING)]),
        "E7-C7",
    );
    assert_eq!(taken, [at("C8")]);
    // Deux d'un coup.
    let pieces = [
        ("C9", ATTACKER),
        ("C8", DEFENDER),
        ("C6", DEFENDER),
        ("C5", ATTACKER),
        ("E7", ATTACKER),
        ("G1", KING),
    ];
    let (_, mut taken) = played(board(&pieces), "E7-C7");
    taken.sort_unstable();
    assert_eq!(taken, [at("C8"), at("C6")]);
}

#[test]
fn only_active_captures_count() {
    let (b, taken) = played(
        board(&[("B7", ATTACKER), ("D7", ATTACKER), ("C9", DEFENDER), ("G1", KING)]),
        "C9-C7",
    );
    assert!(taken.is_empty());
    assert_eq!(b.get(at("C7")), DEFENDER);
}

#[test]
fn the_corners_and_the_throne_take_like_an_enemy_even_when_occupied() {
    // Contre un coin.
    let (_, taken) = played(board(&[("B13", DEFENDER), ("D13", ATTACKER), ("G1", KING)]), "D13-C13");
    assert_eq!(taken, [at("B13")]);
    // Contre le trône vide, et contre le trône occupé par le roi : le serveur prend dans les deux cas.
    let (_, taken) = played(board(&[("G8", DEFENDER), ("G11", ATTACKER), ("A1", KING)]), "G11-G9");
    assert_eq!(taken, [at("G8")]);
    let (_, taken) = played(board(&[("F7", DEFENDER), ("C7", ATTACKER), ("G7", KING)]), "C7-E7");
    assert_eq!(taken, [at("F7")]);
    // Le bord ne prend pas.
    let (_, taken) = played(board(&[("A7", DEFENDER), ("C8", ATTACKER), ("G1", KING)]), "C8-B8");
    assert!(taken.is_empty());
}

#[test]
fn the_king_helps_take_but_is_never_taken_in_between() {
    let (_, taken) = played(board(&[("E7", KING), ("D7", ATTACKER), ("C9", DEFENDER)]), "C9-C7");
    assert_eq!(taken, [at("D7")]);
    let (b, taken) = played(board(&[("D7", KING), ("C7", ATTACKER), ("E9", ATTACKER)]), "E9-E7");
    assert!(taken.is_empty());
    assert!(!b.king_captured());
}

#[test]
fn the_king_falls_when_all_four_sides_are_hostile() {
    // Figure : quatre attaquants.
    let pieces = [
        ("F7", ATTACKER),
        ("H7", ATTACKER),
        ("G8", ATTACKER),
        ("G5", ATTACKER),
        ("G6", KING),
    ];
    let (b, _) = played(board(&pieces), "G5-H5");
    assert!(!b.king_captured());
    let pieces = [
        ("F6", ATTACKER),
        ("H6", ATTACKER),
        ("G7", EMPTY),
        ("I5", ATTACKER),
        ("G6", KING),
    ];
    let (b, _) = played(board(&pieces), "I5-G5");
    assert!(b.king_captured(), "le trône vide en G7 complète l'encerclement");
    assert_eq!(
        Hnefatafl.ending(&b, Color::Red),
        Some(Ending {
            winner: Some(Color::Red),
            reason: Reason::Game("king_captured".into()),
        })
    );
    // Figure : deux attaquants suffisent contre un coin et le bord.
    let (b, _) = played(board(&[("B13", KING), ("B12", ATTACKER), ("E13", ATTACKER)]), "E13-C13");
    assert!(b.king_captured());
}

#[test]
fn only_the_movers_win_is_checked_after_a_move() {
    // Le roi s'enferme lui-même : pas de fin au coup noir, le rouge gagne à son coup suivant.
    let pieces = [
        ("F6", ATTACKER),
        ("H6", ATTACKER),
        ("G5", ATTACKER),
        ("G9", KING),
        ("A3", ATTACKER),
    ];
    let (b, _) = played(board(&pieces), "G9-G6");
    assert!(b.king_captured());
    assert_eq!(Hnefatafl.ending(&b, Color::Black), None);
    let (b, _) = played(b, "A3-A4");
    assert_eq!(Hnefatafl.ending(&b, Color::Red).unwrap().winner, Some(Color::Red));
}

#[test]
fn the_defenders_win_when_the_king_reaches_a_corner() {
    let (b, _) = played(board(&[("A5", KING), ("G3", ATTACKER)]), "A5-A1");
    assert_eq!(
        Hnefatafl.ending(&b, Color::Black),
        Some(Ending {
            winner: Some(Color::Black),
            reason: Reason::Game("king_escaped".into()),
        })
    );
}

#[test]
fn only_the_king_stops_on_the_throne_or_a_corner_others_pass_over_the_empty_throne() {
    let b = board(&[("G10", ATTACKER), ("B13", DEFENDER), ("A9", KING)]);
    let to = |from: &str| {
        let mut all = Vec::new();
        b.destinations(at(from), |t| all.push(cell_name(t)));
        all
    };
    let attacker = to("G10");
    assert!(!attacker.contains(&"G7".into()) && attacker.contains(&"G6".into()) && attacker.contains(&"G1".into()));
    assert!(!to("B13").contains(&"A13".into()));
    let king = board(&[("G9", KING), ("A3", ATTACKER)]);
    let mut all = Vec::new();
    king.destinations(at("G9"), |t| all.push(cell_name(t)));
    assert!(all.contains(&"G7".into()) && all.contains(&"G13".into()));
}

#[test]
fn a_player_without_moves_after_the_opponents_move_is_a_draw() {
    // Le seul attaquant, en A12 sous le coin, est muré.
    let pieces = [("A12", ATTACKER), ("B12", DEFENDER), ("A11", DEFENDER), ("G7", KING)];
    let (b, _) = played(board(&pieces), "G7-G8");
    assert_eq!(
        Hnefatafl.ending(&b, Color::Black),
        Some(Ending {
            winner: None,
            reason: Reason::Game("stalemate".into()),
        })
    );
}

#[test]
fn the_ladder_ends_a_game_that_no_longer_moves_on() {
    let mut b = board(&[("A5", ATTACKER), ("G9", KING)]);
    b.plies = MAX_PLIES - 1;
    let (b, _) = played(b, "A5-A6");
    assert_eq!(
        Hnefatafl.ending(&b, Color::Red).unwrap().reason,
        Reason::Game("max_plies".into())
    );
}

#[test]
fn notation_follows_the_server() {
    for text in ["D6-D5", "D6 - D5", "D6D5", "  D6--D5 ", "D6 D5\r\n"] {
        assert_eq!(mv(text).to_string(), "D6-D5", "{text:?}");
    }
    for text in ["d6-d5", "D6", "D0-D5", "N1-N2", "A14-A13", "D6-D5-D4", ""] {
        assert!(text.parse::<Move>().is_err(), "{text:?}");
    }
    assert_eq!(Hnefatafl.protocol_move(mv("G12-L12")), "G12 - L12");
    assert_eq!((at("A13"), at("M1"), at("G7")), (0, 168, THRONE));
}

#[test]
fn random_games_end_and_their_changes_rebuild_every_position() {
    for seed in 0..20 {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut replay = Replay::new(&Hnefatafl);
        let mut moves = Vec::new();
        while replay.ending.is_none() {
            let m = *Hnefatafl
                .legal_moves(&replay.state, replay.turn)
                .choose(&mut rng)
                .unwrap();
            replay.play(&Hnefatafl, m).unwrap();
            moves.push(m.to_string());
        }
        assert!(moves.len() <= MAX_PLIES as usize);
        let mut cells = Hnefatafl.descriptor().initial;
        for c in replay.changes.iter().flatten() {
            cells[usize::from(c.cell)] = c.code;
        }
        assert_eq!(cells, Hnefatafl.cells(&replay.state), "graine {seed}");
        let ending = replay.ending.clone().unwrap();
        let result = GameResult {
            v: RECORD_VERSION,
            game: Some("hnefatafl".into()),
            game_version: Some(VERSION),
            time_ms: 500,
            opening: vec![],
            moves,
            winner: ending.winner,
            reason: ending.reason,
            invalid: InvalidCounts::default(),
            duration_ms: 1,
            move_ms: vec![],
        };
        assert_eq!(result.validate(&Hnefatafl), Ok(()), "graine {seed}");
        let line = serde_json::to_string(&result).unwrap();
        assert!(line.contains(r#""game":"hnefatafl""#));
    }
}

#[test]
fn a_received_board_reads_back() {
    let b = Board::initial();
    assert_eq!(Hnefatafl.read_cells(&Hnefatafl.cells(&b)), Some(b));
    assert_eq!(Board::from_cells(&[0; CELLS]), None, "sans roi");
    assert_eq!(Board::from_cells(&[0; 10]), None);
}
