//! Duel en mémoire entre deux stratégies, chaque ouverture jouée avec les deux couleurs.
//!
//! `cargo run --profile bot --example duel -- niveau-3 niveau-2 --games 200`

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use rand::SeedableRng;
use rand::rngs::StdRng;

use isolation::board::{ByColor, Color, Move};
use isolation::rules::{self, Replay};
use isolation::strategy::{Bot, Strategy};

#[derive(Parser)]
#[command(name = "duel", about = "Fait jouer deux stratégies l'une contre l'autre, en mémoire")]
struct Cli {
    #[arg(value_enum)]
    first: Strategy,
    #[arg(value_enum)]
    second: Strategy,
    /// Nombre de parties, arrondi au pair supérieur.
    #[arg(long, default_value_t = 100)]
    games: u32,
    /// Temps par coup, comme `--time-ms` de l'arbitre : le dépasser fait perdre.
    #[arg(long, default_value_t = 500)]
    time_ms: u64,
    /// Marge retirée du temps par coup, comme `--margin-ms` du bot.
    #[arg(long, default_value_t = 50)]
    margin_ms: u64,
    /// Demi-coups d'ouverture tirés au hasard, comme `opening_plies` du ladder.
    #[arg(long, default_value_t = 2)]
    opening: usize,
    /// Parties jouées en même temps ; la moitié des coeurs par défaut.
    #[arg(long)]
    jobs: Option<u32>,
    #[arg(long, default_value_t = 1)]
    seed: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Reason {
    NoMoves,
    Timeout,
    Illegal,
}

#[derive(Default)]
struct Depths {
    total: [u64; 2],
    searched: [u64; 2],
}

struct Game {
    index: u32,
    first_color: Color,
    winner: usize,
    reason: Reason,
    plies: usize,
    depths: Depths,
}

fn main() {
    let cli = Cli::parse();
    let pairs = cli.games.div_ceil(2);
    let games = pairs * 2;
    let jobs = cli.jobs.unwrap_or_else(|| {
        let cores = thread::available_parallelism().map_or(2, |n| u32::try_from(n.get()).unwrap_or(2));
        (cores / 2).max(1)
    });
    let players = [cli.first, cli.second];
    let time = Duration::from_millis(cli.time_ms);
    let budget = time.saturating_sub(Duration::from_millis(cli.margin_ms));
    let openings: Vec<Vec<Move>> = (0..u64::from(pairs))
        .map(|pair| rules::random_opening(cli.opening, &mut StdRng::seed_from_u64(cli.seed.wrapping_add(pair))))
        .collect();
    eprintln!(
        "{:?} contre {:?} : {games} parties, {} ms par coup, {jobs} à la fois",
        cli.first, cli.second, cli.time_ms
    );

    let next = AtomicU32::new(0);
    let results = Mutex::new(Vec::new());
    thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= games {
                        break;
                    }
                    let first_color = if index.is_multiple_of(2) {
                        Color::Red
                    } else {
                        Color::Black
                    };
                    let mut rng = StdRng::seed_from_u64(cli.seed ^ (u64::from(index) << 32));
                    let opening = &openings[(index / 2) as usize];
                    let game = play(players, first_color, opening, time, budget, &mut rng, index);
                    report(&game, players);
                    results
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(game);
                }
            });
        }
    });
    let results = results.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    summary(&results, players);
}

fn play(
    players: [Strategy; 2],
    first_color: Color,
    opening: &[Move],
    time: Duration,
    budget: Duration,
    rng: &mut StdRng,
    index: u32,
) -> Game {
    let side = |color: Color| usize::from(color != first_color);
    let mut replay = Replay::from_moves(opening.iter().copied()).unwrap_or_default();
    let mut depths = Depths::default();
    let mut plies = 0;
    let mut bots = players.map(|strategy| Bot::new(strategy, 1));
    let (winner, reason) = loop {
        if let Some(winner) = replay.winner {
            break (side(winner), Reason::NoMoves);
        }
        let turn = replay.turn;
        let me = side(turn);
        let start = Instant::now();
        let choice = bots[me].choose(&replay.board, turn, start + budget, rng);
        if start.elapsed() > time {
            break (1 - me, Reason::Timeout);
        }
        let Some(choice) = choice.filter(|c| rules::is_legal(&replay.board, turn, c.mv)) else {
            break (1 - me, Reason::Illegal);
        };
        if let Some(depth) = choice.depth {
            depths.total[me] += u64::from(depth);
            depths.searched[me] += 1;
        }
        replay.board.apply(choice.mv);
        replay.winner = rules::winner_after(&replay.board, turn);
        replay.turn = turn.other();
        plies += 1;
    };
    Game {
        index,
        first_color,
        winner,
        reason,
        plies,
        depths,
    }
}

fn report(game: &Game, players: [Strategy; 2]) {
    let colors = ByColor {
        red: "rouge",
        black: "noir",
    };
    let winner_color = if game.winner == 0 {
        game.first_color
    } else {
        game.first_color.other()
    };
    eprintln!(
        "partie {:>4} : {:?} ({}) gagne en {} coups, {:?}",
        game.index + 1,
        players[game.winner],
        colors[winner_color],
        game.plies,
        game.reason
    );
}

fn summary(results: &[Game], players: [Strategy; 2]) {
    let n = results.len();
    let wins = results.iter().filter(|g| g.winner == 0).count();
    let as_red = results
        .iter()
        .filter(|g| g.first_color == Color::Red)
        .map(|g| u32::from(g.winner == 0))
        .sum::<u32>();
    let losses_by = |reason| results.iter().filter(|g| g.reason == reason && g.winner == 1).count();
    let wins_by = |reason| results.iter().filter(|g| g.reason == reason && g.winner == 0).count();
    #[expect(clippy::cast_precision_loss, reason = "quelques milliers de parties au plus")]
    let (score, n_f) = (wins as f64 / n as f64, n as f64);
    let margin = 1.96 * (score * (1.0 - score) / n_f).sqrt();
    println!(
        "{:?} contre {:?} : {wins} victoires sur {n} ({} en rouge), score {:.1} % ± {:.1}",
        players[0],
        players[1],
        as_red,
        100.0 * score,
        100.0 * margin
    );
    println!(
        "écart d'Elo : {} (intervalle à 95 % : {} à {})",
        elo(score),
        elo(score - margin),
        elo(score + margin)
    );
    for (side, strategy) in players.iter().enumerate() {
        let total: u64 = results.iter().map(|g| g.depths.total[side]).sum();
        let searched: u64 = results.iter().map(|g| g.depths.searched[side]).sum();
        #[expect(clippy::cast_precision_loss, reason = "moyenne affichée")]
        let average = if searched == 0 {
            0.0
        } else {
            total as f64 / searched as f64
        };
        let defeats = [Reason::Timeout, Reason::Illegal].map(|r| if side == 0 { losses_by(r) } else { wins_by(r) });
        println!(
            "{strategy:?} : profondeur moyenne {average:.2}, timeouts {}, coups illégaux {}",
            defeats[0], defeats[1]
        );
    }
}

fn elo(score: f64) -> String {
    if score <= 0.0 {
        "-inf".to_string()
    } else if score >= 1.0 {
        "+inf".to_string()
    } else {
        format!("{:+.0}", -400.0 * (1.0 / score - 1.0).log10())
    }
}
