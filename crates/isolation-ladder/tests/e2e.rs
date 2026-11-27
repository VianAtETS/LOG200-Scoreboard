//! Une partie complète entre les binaires compilés : arbitre et deux bots.

#![allow(clippy::unwrap_used)]

use std::process::{Command, Stdio};

use isolation_core::board::{Board, Color};
use isolation_core::record::{GameResult, RefereeEvent};
use isolation_core::rules::legal_moves;

mod support;

/// Le résultat, et la sortie d'erreur de chaque bot (rouge puis noir).
fn play(bot_args: &[&str], opening: &str) -> (GameResult, [String; 2]) {
    let port = support::free_port().to_string();
    let bot_exe = support::binary("isolation-bot");
    let referee = Command::new(support::binary("isolation-referee"))
        .args(["--port", &port, "--time-ms", "2000", "--opening", opening])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let bot = |seed: &str| {
        Command::new(&bot_exe)
            .args(["--port", &port, "--seed", seed])
            .args(bot_args)
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let red = bot("1");
    // L'arbitre attribue les couleurs dans l'ordre de connexion.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let black = bot("2");

    let output = referee.wait_with_output().unwrap();
    assert!(output.status.success());
    let stderr = |bot: std::process::Child, side: &str| {
        let output = bot.wait_with_output().unwrap();
        assert!(output.status.success(), "divergence vue par le {side}");
        String::from_utf8(output.stderr).unwrap()
    };
    let logs = [stderr(red, "rouge"), stderr(black, "noir")];
    let text = String::from_utf8(output.stdout).unwrap();
    let (result, events) = text
        .trim_end()
        .lines()
        .collect::<Vec<_>>()
        .split_last()
        .map(|(r, e)| (*r, e.to_vec()))
        .unwrap();
    let result: GameResult = serde_json::from_str(result).unwrap();
    let events: Vec<RefereeEvent> = events.iter().map(|line| serde_json::from_str(line).unwrap()).collect();
    assert_eq!(events.first(), Some(&RefereeEvent::Ready));
    let moves: Vec<&str> = events[1..]
        .iter()
        .map(|event| match event {
            RefereeEvent::Move { mv, .. } => mv.as_str(),
            RefereeEvent::Ready => panic!("ready en double"),
        })
        .collect();
    assert_eq!(moves, result.moves, "un événement par coup client, dans l'ordre");
    (result, logs)
}

#[test]
fn searching_bots_print_one_arena_line_per_move() {
    let (result, logs) = play(&["--strategy", "niveau-1"], "");
    let red_moves = result.moves.len().div_ceil(2);
    for (log, moves) in logs.iter().zip([red_moves, result.moves.len() / 2]) {
        let arena: Vec<&str> = log.lines().filter(|l| l.starts_with("ARENA ")).collect();
        assert_eq!(arena.len(), moves, "{log}");
        assert!(arena.iter().any(|l| l.contains("depth=")), "{log}");
    }
}

#[test]
fn bots_play_a_full_valid_game() {
    let (result, _) = play(&["--strategy", "greedy"], "");
    assert!(result.validate(&isolation_core::Isolation).is_ok());
    assert!(result.opening.is_empty());
    assert!(!result.moves.is_empty());
    assert_eq!(result.move_ms.len(), result.moves.len());
}

#[test]
fn opening_is_canonicalised_and_probes_find_no_divergence() {
    let mut board = Board::initial();
    let first = legal_moves(&board, Color::Red)[0];
    board.apply(first);
    let second = legal_moves(&board, Color::Black)[0];
    let loose = format!("{} , {second}", first.to_string().replace('-', " ").to_lowercase());

    let (result, _) = play(&["--strategy", "random", "--probe-illegal"], &loose);
    assert!(result.validate(&isolation_core::Isolation).is_ok());
    assert_eq!(result.opening, [first.to_string(), second.to_string()]);
    assert!(
        result.invalid.red > 0 && result.invalid.black > 0,
        "les sondes doivent être refusées"
    );
}
