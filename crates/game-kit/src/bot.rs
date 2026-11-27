//! Un client du protocole pour n'importe quel `Game` : il joue au hasard parmi les coups légaux. C'est
//! l'adversaire des smoke tests d'un jeu sans bot propre, et de quoi tester un arbitre ou un jar à la
//! main. Il suit les deux dialectes du serveur officiel.
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     game_kit::bot::main(&MonJeu, "mon-jeu-bot", "Joue une partie de mon jeu au hasard")
//! }
//! ```

use std::net::TcpStream;
use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::{CommandFactory, FromArgMatches, Parser};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;

use crate::game::Game;
use crate::protocol::LineConn;
use crate::side::Color;

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    #[arg(long, default_value_t = 8888)]
    port: u16,
    /// `aleatoire` : un coup légal au hasard. `premier` : le premier coup légal, pour un test reproductible.
    #[arg(long, default_value = "aleatoire")]
    strategy: String,
    /// Graine du hasard ; absente, une graine au hasard.
    #[arg(long)]
    seed: Option<u64>,
    /// Délai pour se connecter à l'arbitre.
    #[arg(long, default_value_t = 10_000)]
    connect_timeout_ms: u64,
    /// Conformance : signale chaque écart entre l'arbitre (le serveur officiel) et nos règles, et sort
    /// en code 1 s'il y en a eu.
    #[arg(long)]
    check: bool,
    /// Quitte la partie après autant de demi-coups (les deux joueurs) : le serveur officiel ne borne pas
    /// une partie qui n'avance plus.
    #[arg(long)]
    max_plies: Option<usize>,
}

/// Point d'entrée du bot d'un jeu. Code 0 à la fin normale de la partie, 1 si l'arbitre refuse un
/// coup (le bot et l'arbitre divergent), 2 en cas d'erreur.
pub fn main<G: Game>(game: &G, name: &'static str, about: &'static str) -> ExitCode {
    let command = Cli::command()
        .name(name)
        .bin_name(name)
        .about(about)
        .version(env!("CARGO_PKG_VERSION"));
    let cli = match Cli::from_arg_matches(&command.get_matches()) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match play(game, &cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(2)
        }
    }
}

fn connect(cli: &Cli) -> anyhow::Result<TcpStream> {
    let deadline = Instant::now() + Duration::from_millis(cli.connect_timeout_ms);
    loop {
        match TcpStream::connect((cli.host.as_str(), cli.port)) {
            Ok(stream) => return Ok(stream),
            Err(e) if Instant::now() < deadline => {
                let _ = e;
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e).with_context(|| format!("connexion à {}:{}", cli.host, cli.port)),
        }
    }
}

/// Vrai si la partie s'est terminée normalement et, avec `--check`, sans divergence avec nos règles.
fn play<G: Game>(game: &G, cli: &Cli) -> anyhow::Result<bool> {
    let mut rng = cli.seed.map_or_else(rand::make_rng, StdRng::seed_from_u64);
    let mut conn = LineConn::with_dialect(connect(cli)?, game.dialect()).context("socket")?;
    let mut state = game.initial();
    let mut side = Color::Red;
    // Avec --check : la fin que nos règles voient après notre dernier coup, et les écarts relevés.
    let mut ours_ended = None;
    let mut divergences = 0;
    let mut plies = 0;
    let mut diverge = |what: String| {
        eprintln!("DIVERGENCE : {what}");
        divergences += 1;
    };
    loop {
        let message = match conn.recv(None) {
            Ok(message) => message,
            Err(e) => bail!("arbitre : {e}"),
        };
        let (code, rest) = message.split_once(' ').unwrap_or((&message, ""));
        match code.trim() {
            // Début de partie (dialecte actuel) : le dernier champ est notre couleur.
            "0" => {
                if rest.split_whitespace().last() == Some("1") {
                    side = Color::Black;
                }
            }
            "1" | "2" => {
                side = if code.trim() == "1" { Color::Red } else { Color::Black };
                let cells: Vec<u8> = rest.split_whitespace().filter_map(|c| c.parse().ok()).collect();
                state = game.read_cells(&cells).unwrap_or_else(|| game.initial());
                if side == Color::Red {
                    ours_ended = play_one(game, &mut conn, &mut state, side, &cli.strategy, &mut rng)?;
                    plies += 1;
                }
            }
            "3" => {
                if cli.check
                    && let Some(reason) = ours_ended.take()
                {
                    diverge(format!(
                        "la partie continue alors que nos règles l'ont finie ({reason})"
                    ));
                }
                // Le coup adverse ; illisible ou absent (`A0-A0`), rien à rejouer.
                if let Ok(mv) = rest.trim().parse::<G::Move>() {
                    if game.is_legal(&state, side.other(), mv) {
                        game.apply(&mut state, side.other(), mv);
                        plies += 1;
                        if cli.check
                            && let Some(end) = game.ending(&state, side.other())
                        {
                            diverge(format!("après {mv}, nos règles finissent la partie ({})", end.reason));
                        }
                    } else if cli.check {
                        diverge(format!("coup adverse illégal selon nos règles : {mv}"));
                    }
                }
                if cli.max_plies.is_some_and(|max| plies >= max) {
                    eprintln!("{plies} demi-coups : arrêt, {divergences} divergences");
                    return Ok(divergences == 0);
                }
                ours_ended = play_one(game, &mut conn, &mut state, side, &cli.strategy, &mut rng)?;
                plies += 1;
            }
            "4" => {
                eprintln!("DIVERGENCE : coup refusé par l'arbitre");
                return Ok(false);
            }
            "5" => {
                eprintln!("fin de partie : {message}");
                if cli.check {
                    // Le dernier coup : le nôtre, déjà joué, ou celui de l'adversaire.
                    let theirs = rest
                        .trim()
                        .parse::<G::Move>()
                        .ok()
                        .filter(|&mv| game.is_legal(&state, side.other(), mv));
                    let end = match theirs {
                        Some(mv) => {
                            game.apply(&mut state, side.other(), mv);
                            game.ending(&state, side.other()).map(|end| end.reason)
                        }
                        None => ours_ended.take(),
                    };
                    match end {
                        Some(reason) => eprintln!("nos règles : fin ({reason})"),
                        None => diverge("fin annoncée que nos règles ne voient pas".into()),
                    }
                }
                return Ok(divergences == 0);
            }
            _ => eprintln!("message ignoré : {message:?}"),
        }
    }
}

/// Joue un coup ; rend la raison de fin que nos règles voient après lui, s'il y en a une.
fn play_one<G: Game>(
    game: &G,
    conn: &mut LineConn,
    state: &mut G::State,
    side: Color,
    strategy: &str,
    rng: &mut StdRng,
) -> anyhow::Result<Option<crate::record::Reason>> {
    let moves = game.legal_moves(state, side);
    let chosen = match strategy {
        "premier" => moves.first(),
        _ => moves.choose(rng),
    };
    let Some(&mv) = chosen else {
        bail!("aucun coup légal");
    };
    game.apply(state, side, mv);
    conn.send(mv).context("envoi du coup")?;
    Ok(game.ending(state, side).map(|end| end.reason))
}
