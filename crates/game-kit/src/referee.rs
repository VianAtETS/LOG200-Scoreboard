//! Arbitre d'une partie avec le protocole du serveur officiel, pour n'importe quel `Game`. Le
//! contrat avec le ladder (marqueurs sur stderr, événements puis résultat JSON sur stdout,
//! sous-commandes) est dans docs/formats.md et docs/spec-multi-jeux.md.
//!
//! L'exécutable d'un jeu se réduit à :
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     game_kit::referee::main(&MonJeu, "mon-jeu-referee", "Arbitre une partie de mon jeu")
//! }
//! ```

use std::io::{self, BufRead, IsTerminal, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::Serialize;

use crate::game::{Change, Game};
use crate::protocol::{self as wire, LineConn, Outcome, RecvError};
use crate::record::{GameResult, InvalidCounts, RECORD_VERSION, Reason, RefereeEvent};
use crate::replay::Replay;
use crate::side::{ByColor, Color};

#[derive(Parser)]
#[command(args_conflicts_with_subcommands = true, subcommand_negates_reqs = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    play: PlayArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Écrit le descripteur du jeu (JSON, une ligne).
    Describe,
    /// Écrit une ouverture aléatoire reproductible, au format de `--opening`.
    Opening {
        /// Demi-coups, en nombre pair.
        #[arg(long)]
        plies: usize,
        #[arg(long)]
        seed: u64,
    },
    /// Valide des résultats lus sur stdin (un JSON par ligne) et écrit leurs effets.
    Check,
}

#[derive(Args)]
struct PlayArgs {
    #[arg(long, required = true)]
    port: Option<u16>,
    /// Temps par coup.
    #[arg(long, required = true)]
    time_ms: Option<u64>,
    /// Demi-coups imposés avant la main aux joueurs, en nombre pair, séparés par des virgules.
    #[arg(long, default_value = "")]
    opening: String,
    /// Coups invalides tolérés avant défaite ; illimité par défaut.
    #[arg(long, default_value_t = u32::MAX, hide_default_value = true)]
    max_invalid: u32,
    /// Délai de connexion, pour chaque joueur.
    #[arg(long, default_value_t = 30_000)]
    connect_timeout_ms: u64,
    /// Tolérance ajoutée au premier coup de chaque joueur, le temps d'une JVM à froid.
    #[arg(long, default_value_t = 100)]
    first_move_grace_ms: u64,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
    /// Marge du rouge sur chaque coup, à son insu : le temps que le profileur lui prend. Les joueurs
    /// reçoivent toujours `time_ms`.
    #[arg(long, env = "ISOLATION_EXTRA_RED_MS", default_value_t = 0)]
    extra_red_ms: u64,
    /// Marge du noir, comme `extra_red_ms`.
    #[arg(long, env = "ISOLATION_EXTRA_BLACK_MS", default_value_t = 0)]
    extra_black_ms: u64,
}

/// Point d'entrée de l'arbitre d'un jeu. Code 2 en cas d'erreur de l'arbitre lui-même.
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
    let done = match cli.command {
        Some(Command::Describe) => describe(game),
        Some(Command::Opening { plies, seed }) => opening(game, plies, seed),
        Some(Command::Check) => check(game),
        None => play(game, &cli.play),
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(2)
        }
    }
}

fn describe<G: Game>(game: &G) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string(&game.descriptor())?);
    Ok(())
}

fn opening<G: Game>(game: &G, plies: usize, seed: u64) -> anyhow::Result<()> {
    if plies % 2 == 1 {
        bail!("l'ouverture doit compter un nombre pair de demi-coups");
    }
    let moves = game.random_opening(plies, &mut StdRng::seed_from_u64(seed));
    println!("{}", join(&moves));
    Ok(())
}

/// Coups séparés par des virgules, comme l'option `--opening`.
pub fn join<M: std::fmt::Display>(moves: &[M]) -> String {
    moves.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

#[derive(Serialize)]
#[serde(untagged)]
enum Checked {
    Ok { ok: bool, changes: Vec<Vec<Change>> },
    Err { ok: bool, error: String },
}

fn check<G: Game>(game: &G) -> anyhow::Result<()> {
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line.context("lecture de stdin")?;
        if line.trim().is_empty() {
            continue;
        }
        let checked = match serde_json::from_str::<GameResult>(&line) {
            Ok(result) => match result.replay(game) {
                Ok(replay) => Checked::Ok {
                    ok: true,
                    changes: replay.changes,
                },
                Err(e) => Checked::Err {
                    ok: false,
                    error: e.to_string(),
                },
            },
            Err(e) => Checked::Err {
                ok: false,
                error: format!("résultat illisible : {e}"),
            },
        };
        writeln!(out, "{}", serde_json::to_string(&checked)?)?;
        out.flush()?;
    }
    Ok(())
}

fn play<G: Game>(game: &G, args: &PlayArgs) -> anyhow::Result<()> {
    let (Some(port), Some(time_ms)) = (args.port, args.time_ms) else {
        bail!("--port et --time-ms sont obligatoires");
    };
    let opening: Vec<G::Move> = parse_opening(&args.opening)?;
    let state = opening_state(game, &opening)?;
    let listener =
        TcpListener::bind((args.bind.as_str(), port)).with_context(|| format!("écoute sur {}:{port}", args.bind))?;
    eprintln!("LISTENING {port}");

    let descriptor = game.descriptor();
    let size = (u32::from(descriptor.rows), u32::from(descriptor.cols));
    let start = Instant::now();
    let connect_timeout = Duration::from_millis(args.connect_timeout_ms);
    let cells = game.cells(&state);
    let (verdict, moves, move_ms, invalid) = match seat_players(&listener, connect_timeout, size, time_ms, &cells)? {
        Lobby::Ready(players) => {
            let mut referee = Referee {
                game,
                state,
                players,
                time: Duration::from_millis(time_ms),
                first_move_grace: Duration::from_millis(args.first_move_grace_ms),
                extra: ByColor {
                    red: Duration::from_millis(args.extra_red_ms),
                    black: Duration::from_millis(args.extra_black_ms),
                },
                max_invalid: args.max_invalid,
                moves: Vec::new(),
                move_ms: Vec::new(),
                invalid: InvalidCounts::default(),
            };
            announce(&RefereeEvent::Ready);
            let verdict = referee.run();
            referee.players.red.shutdown();
            referee.players.black.shutdown();
            (verdict, referee.moves, referee.move_ms, referee.invalid)
        }
        Lobby::NoShow { absent, red } => {
            // Le rouge déjà connecté apprend qu'il gagne, sans coup joué.
            if let Some(mut red) = red {
                let _ = red.send(wire::end(Outcome::Win, None));
                red.shutdown();
            }
            let verdict = Verdict {
                winner: absent.other(),
                reason: Reason::NoShow,
            };
            (verdict, Vec::new(), Vec::new(), InvalidCounts::default())
        }
    };

    let result = GameResult {
        v: RECORD_VERSION,
        time_ms,
        opening: opening.iter().map(ToString::to_string).collect(),
        moves: moves.iter().map(ToString::to_string).collect(),
        winner: verdict.winner,
        reason: verdict.reason,
        invalid,
        duration_ms: millis(start),
        move_ms,
    };
    eprintln!(
        "gagnant {} ({:?}) en {} coups",
        verdict.winner,
        verdict.reason,
        result.moves.len()
    );
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn parse_opening<M: std::str::FromStr>(text: &str) -> anyhow::Result<Vec<M>> {
    text.split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| {
            m.parse()
                .map_err(|_| anyhow::anyhow!("ouverture : coup illisible {m:?}"))
        })
        .collect()
}

fn opening_state<G: Game>(game: &G, opening: &[G::Move]) -> anyhow::Result<G::State> {
    let replay = Replay::from_moves(game, opening.iter().copied()).context("ouverture")?;
    if replay.ending.is_some() {
        bail!("l'ouverture termine la partie");
    }
    if replay.turn != Color::Red {
        bail!("l'ouverture doit compter un nombre pair de demi-coups");
    }
    Ok(replay.state)
}

fn announce(event: &RefereeEvent) {
    if io::stdout().is_terminal() {
        return;
    }
    if let Ok(line) = serde_json::to_string(event) {
        println!("{line}");
    }
}

fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

enum Lobby {
    Ready(ByColor<LineConn>),
    NoShow { absent: Color, red: Option<LineConn> },
}

fn seat_players(
    listener: &TcpListener,
    timeout: Duration,
    (rows, cols): (u32, u32),
    time_ms: u64,
    cells: &[u8],
) -> anyhow::Result<Lobby> {
    let seat = |color| -> anyhow::Result<Option<LineConn>> {
        let Some(stream) = accept(listener, Instant::now() + timeout).context("attente des joueurs")? else {
            eprintln!("{color} ne s'est pas connecté");
            return Ok(None);
        };
        let mut conn = LineConn::new(stream).context("socket")?;
        eprintln!("CONNECTED {color}");
        let _ = conn.send(wire::new_game(rows, cols, time_ms, color));
        if color == Color::Black {
            let _ = conn.send(wire::board(color, cells));
        }
        Ok(Some(conn))
    };
    let Some(red) = seat(Color::Red)? else {
        return Ok(Lobby::NoShow {
            absent: Color::Red,
            red: None,
        });
    };
    Ok(match seat(Color::Black)? {
        Some(black) => Lobby::Ready(ByColor { red, black }),
        None => Lobby::NoShow {
            absent: Color::Black,
            red: Some(red),
        },
    })
}

fn accept(listener: &TcpListener, deadline: Instant) -> io::Result<Option<TcpStream>> {
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(Some(stream));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
}

/// `played` ne compte que les coups des clients : le rouge joue le coup 0, le noir le coup 1.
fn allowed(time: Duration, grace: Duration, extra: Duration, played: usize) -> Duration {
    extra + if played < 2 { time + grace } else { time }
}

struct Verdict {
    winner: Color,
    reason: Reason,
}

struct Referee<'g, G: Game> {
    game: &'g G,
    state: G::State,
    players: ByColor<LineConn>,
    time: Duration,
    first_move_grace: Duration,
    /// Marge de chaque couleur, ajoutée à chacun de ses coups.
    extra: ByColor<Duration>,
    max_invalid: u32,
    moves: Vec<G::Move>,
    move_ms: Vec<u64>,
    invalid: InvalidCounts,
}

impl<G: Game> Referee<'_, G> {
    fn send(&mut self, color: Color, line: &str) -> bool {
        self.players[color].send(line).is_ok()
    }

    fn run(&mut self) -> Verdict {
        let mut turn = Color::Red;
        loop {
            let request = match self.moves.last() {
                None => wire::board(Color::Red, &self.game.cells(&self.state)),
                Some(&last) => wire::request(Some(&self.game.protocol_move(last))),
            };
            // Temps compté contre le joueur : réseau et coups refusés compris.
            let asked = Instant::now();
            if !self.send(turn, &request) {
                return self.forfeit(turn, Reason::Disconnect);
            }
            let mv = match self.receive_move(turn) {
                Ok(mv) => mv,
                Err(reason) => return self.forfeit(turn, reason),
            };
            let changes = self.game.apply(&mut self.state, turn, mv);
            self.moves.push(mv);
            let ms = millis(asked);
            self.move_ms.push(ms);
            announce(&RefereeEvent::Move {
                mv: mv.to_string(),
                ms,
                changes,
            });
            if let Some(ending) = self.game.ending(&self.state, turn) {
                let last = self.game.protocol_move(mv);
                for (color, outcome) in [(ending.winner, Outcome::Win), (ending.winner.other(), Outcome::Loss)] {
                    self.send(color, &wire::end(outcome, Some(&last)));
                }
                return Verdict {
                    winner: ending.winner,
                    reason: ending.reason,
                };
            }
            turn = turn.other();
        }
    }

    /// Premier coup légal de `turn` avant l'échéance ; chaque coup refusé est répondu par `4`.
    fn receive_move(&mut self, turn: Color) -> Result<G::Move, Reason> {
        let deadline = Instant::now() + allowed(self.time, self.first_move_grace, self.extra[turn], self.moves.len());
        loop {
            let line = match self.players[turn].recv(Some(deadline)) {
                Ok(line) => line,
                Err(RecvError::Timeout) => return Err(Reason::Timeout),
                Err(_) => return Err(Reason::Disconnect),
            };
            match line.parse() {
                Ok(mv) if self.game.is_legal(&self.state, turn, mv) => return Ok(mv),
                _ => {
                    self.invalid[turn] += 1;
                    eprintln!("coup invalide de {turn} : {line:?}");
                    if self.invalid[turn] >= self.max_invalid {
                        return Err(Reason::Protocol);
                    }
                    if !self.send(turn, wire::INVALID) {
                        return Err(Reason::Disconnect);
                    }
                }
            }
        }
    }

    /// Comme le serveur officiel : `5 TIMEOUT` aux deux joueurs pour un temps dépassé, sinon WIN ou
    /// LOSS ; jamais de dernier coup.
    fn forfeit(&mut self, loser: Color, reason: Reason) -> Verdict {
        let winner = loser.other();
        for (color, outcome) in [(winner, Outcome::Win), (loser, Outcome::Loss)] {
            let outcome = if reason == Reason::Timeout {
                Outcome::Timeout
            } else {
                outcome
            };
            self.send(color, &wire::end(outcome, None));
        }
        Verdict { winner, reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_each_players_first_move_gets_the_grace() {
        let (time, grace) = (Duration::from_millis(500), Duration::from_millis(1000));
        let with_grace = Duration::from_millis(1500);
        assert_eq!(allowed(time, grace, Duration::ZERO, 0), with_grace);
        assert_eq!(allowed(time, grace, Duration::ZERO, 1), with_grace);
        assert_eq!(allowed(time, grace, Duration::ZERO, 2), time);
    }

    #[test]
    fn the_margin_of_a_colour_widens_each_of_its_moves() {
        let (time, grace, extra) = (
            Duration::from_millis(500),
            Duration::from_millis(1000),
            Duration::from_millis(25),
        );
        assert_eq!(allowed(time, grace, extra, 0), Duration::from_millis(1525));
        assert_eq!(allowed(time, grace, extra, 7), Duration::from_millis(525));
        let cli = Cli::try_parse_from([
            "isolation-referee",
            "--port",
            "1",
            "--time-ms",
            "500",
            "--extra-black-ms",
            "25",
        ])
        .unwrap();
        assert_eq!((cli.play.extra_red_ms, cli.play.extra_black_ms), (0, 25));
    }

    #[test]
    fn the_cli_accepts_play_flags_or_a_subcommand() {
        Cli::command().debug_assert();
        let play = Cli::try_parse_from(["r", "--port", "8888", "--time-ms", "500"]).unwrap();
        assert!(play.command.is_none());
        assert_eq!((play.play.port, play.play.time_ms), (Some(8888), Some(500)));
        let opening = Cli::try_parse_from(["r", "opening", "--plies", "2", "--seed", "7"]).unwrap();
        assert!(matches!(opening.command, Some(Command::Opening { plies: 2, seed: 7 })));
        assert!(Cli::try_parse_from(["r", "describe"]).is_ok());
        assert!(Cli::try_parse_from(["r", "--time-ms", "500"]).is_err(), "port manquant");
    }
}
