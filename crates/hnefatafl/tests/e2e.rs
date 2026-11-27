//! Une partie complète entre les binaires compilés, dans le dialecte de l'ancien serveur : arbitre et
//! deux bots, sans aucune fin de ligne sur le fil.

#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::time::Duration;

use game_kit::record::{GameResult, RefereeEvent};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn referee(port: u16, opening: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_hnefatafl-referee"))
        .args(["--port", &port.to_string(), "--time-ms", "2000", "--opening", opening])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn result_of(output: &std::process::Output) -> (GameResult, Vec<RefereeEvent>) {
    let text = String::from_utf8(output.stdout.clone()).unwrap();
    let lines: Vec<&str> = text.trim_end().lines().collect();
    let (result, events) = lines.split_last().unwrap();
    let events = events.iter().map(|l| serde_json::from_str(l).unwrap()).collect();
    (serde_json::from_str(result).unwrap(), events)
}

#[test]
fn two_random_bots_play_a_full_valid_game() {
    let port = free_port();
    let referee = referee(port, "");
    let bot = |seed: &str| {
        Command::new(env!("CARGO_BIN_EXE_hnefatafl-bot"))
            .args(["--port", &port.to_string(), "--seed", seed])
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut red = bot("1");
    std::thread::sleep(Duration::from_millis(300));
    let mut black = bot("2");
    let output = referee.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(
        red.wait().unwrap().success() && black.wait().unwrap().success(),
        "un bot a divergé"
    );
    let (result, events) = result_of(&output);
    assert_eq!(result.validate(&hnefatafl::Hnefatafl), Ok(()));
    assert_eq!(
        (result.game.as_deref(), result.game_version),
        (Some("hnefatafl"), Some(1))
    );
    assert_eq!(events.len(), result.moves.len() + 1, "ready puis un événement par coup");
}

/// Le fil, octet pour octet, côté rouge : ce qu'un jar écrit pour l'ancien serveur.
#[test]
fn the_wire_is_the_old_servers() {
    let port = free_port();
    let referee = referee(port, "");
    let connect = || loop {
        if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
            break s;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut red = connect();
    std::thread::sleep(Duration::from_millis(200));
    let mut black = connect();
    let receive = |s: &mut TcpStream| {
        let mut buffer = [0; 2048];
        let n = s.read(&mut buffer).unwrap();
        String::from_utf8_lossy(&buffer[..n]).to_string()
    };
    let to_black = receive(&mut black);
    assert!(to_black.starts_with("2 0 0 0 0 4 4 4 4 4 0 0 0 0 0"), "{to_black:?}");
    assert!(!to_black.contains('\n'), "pas de fin de ligne");
    let to_red = receive(&mut red);
    assert!(to_red.starts_with("1 0 0 0 0 4"), "{to_red:?}");
    // Un coup sans fin de ligne, au format compact.
    red.write_all(b"E13E10").unwrap();
    assert_eq!(receive(&mut black), "3 E13 - E10");
    // Un coup illégal est refusé par 4.
    black.write_all(b"G10-G13").unwrap();
    assert_eq!(receive(&mut black), "4");
    black.write_all(b"G10 - L10").unwrap();
    assert_eq!(receive(&mut red), "3 G10 - L10");
    drop(red);
    let output = referee.wait_with_output().unwrap();
    let (result, _) = result_of(&output);
    assert_eq!(result.moves, ["E13-E10", "G10-L10"]);
    assert_eq!(result.invalid.black, 1);
    assert_eq!(
        receive(&mut black),
        "5",
        "fin sans résultat ni coup : le rouge s'est déconnecté"
    );
}
