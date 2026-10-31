//! TEMP : rejoue une partie enregistrée et chronomètre chaque coup de niveau-5, ponder compris.
//! `cargo run --profile bot --example stall -- red|black G1-G3-F9,F12-E12-J6,... [répétitions]`

use std::time::{Duration, Instant};

use rand::SeedableRng;
use rand::rngs::StdRng;

use isolation::board::{Color, Move};
use isolation::engine::{Engine, Eval};
use isolation::rules::Replay;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let own = if args[1] == "red" { Color::Red } else { Color::Black };
    let moves: Vec<Move> = args[2].split(',').map(|m| m.parse().unwrap()).collect();
    let reps: u64 = args.get(3).map_or(1, |r| r.parse().unwrap());
    let budget = Duration::from_millis(950);
    for rep in 0..reps {
        let mut rng = StdRng::seed_from_u64(rep);
        let mut engine = Engine::new(Eval::Territory, 1);
        let mut worst = (Duration::ZERO, 0);
        for ply in 2..=moves.len() {
            let replay = Replay::from_moves(moves[..ply].iter().copied()).unwrap();
            if replay.turn != own || replay.winner.is_some() {
                continue;
            }
            // ponder pendant le coup adverse, comme le bot
            let before = Replay::from_moves(moves[..ply - 1].iter().copied()).unwrap();
            engine.ponder(&before.board, own.other());
            std::thread::sleep(budget);
            let start = Instant::now();
            engine.choose(&replay.board, own, start + budget, &mut rng);
            let elapsed = start.elapsed();
            if elapsed > worst.0 {
                worst = (elapsed, ply);
            }
            if elapsed > Duration::from_millis(990) {
                println!("rep {rep} ply {ply} : {elapsed:?}");
            }
        }
        println!("rep {rep} : pire {:?} au demi-coup {}", worst.0, worst.1);
    }
}
