//! Arbitre d'une partie avec le protocole du serveur officiel. Le contrat avec le ladder (marqueurs
//! sur stderr, événements puis résultat JSON sur stdout) est dans docs/formats.md.

use std::io::{self, IsTerminal};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;

use isolation_core::board::{Board, ByColor, Color, Move, SIZE};
use isolation_core::protocol::{LineConn, Outcome, RecvError, ServerMsg};
use isolation_core::record::{GameResult, InvalidCounts, RECORD_VERSION, Reason, RefereeEvent};
use isolation_core::rules::{self, Replay};

#[derive(Parser)]
#[command(
    name = "isolation-referee",
    version,
    about = "Arbitre une partie d'Isolation entre deux clients TCP"
)]
struct Cli {
    #[arg(long)]
    port: u16,
    /// Temps par coup.
    #[arg(long)]
    time_ms: u64,
    /// Demi-coups imposés avant la main aux joueurs, en nombre pair : `G1-G4-F9,F12-F10-G7`.
    #[arg(long, default_value = "", value_parser = parse_opening)]
    opening: Opening,
    /// Coups invalides tolérés avant défaite ; illimité par défaut.
    #[arg(long, default_value_t = u32::MAX, hide_default_value = true)]
    max_invalid: u32,
    /// Délai de connexion, pour chaque joueur.
    #[arg(long, default_value_t = 30_000)]
    connect_timeout_ms: u64,
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,
}

#[derive(Clone)]
struct Opening(Vec<Move>);

fn parse_opening(text: &str) -> Result<Opening, String> {
    let moves = text
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| m.parse().map_err(|e| format!("{e}")))
        .collect::<Result<Vec<Move>, _>>()?;
    Ok(Opening(moves))
}

fn main() -> ExitCode {
    match run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: &Cli) -> anyhow::Result<()> {
    let board = opening_board(&cli.opening.0)?;
    let listener = TcpListener::bind((cli.bind.as_str(), cli.port))
        .with_context(|| format!("écoute sur {}:{}", cli.bind, cli.port))?;
    eprintln!("LISTENING {}", cli.port);

    let start = Instant::now();
    let connect_timeout = Duration::from_millis(cli.connect_timeout_ms);
    let (verdict, moves, move_ms, invalid) = match seat_players(&listener, connect_timeout, cli.time_ms, &board)? {
        Lobby::Ready(players) => {
            let mut referee = Referee {
                board,
                players,
                time: Duration::from_millis(cli.time_ms),
                max_invalid: cli.max_invalid,
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
                let _ = red.send(ServerMsg::End {
                    outcome: Outcome::Win,
                    last: None,
                });
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
        time_ms: cli.time_ms,
        opening: cli.opening.0.iter().map(Move::to_string).collect(),
        moves: moves.iter().map(Move::to_string).collect(),
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

fn opening_board(opening: &[Move]) -> anyhow::Result<Board> {
    let replay = Replay::from_moves(opening.iter().copied()).context("ouverture")?;
    if replay.winner.is_some() {
        bail!("l'ouverture termine la partie");
    }
    if replay.turn != Color::Red {
        bail!("l'ouverture doit compter un nombre pair de demi-coups");
    }
    Ok(replay.board)
}

enum Lobby {
    Ready(ByColor<LineConn>),
    NoShow { absent: Color, red: Option<LineConn> },
}

fn seat_players(listener: &TcpListener, timeout: Duration, time_ms: u64, board: &Board) -> anyhow::Result<Lobby> {
    let seat = |color| -> anyhow::Result<Option<LineConn>> {
        let Some(stream) = accept(listener, Instant::now() + timeout).context("attente des joueurs")? else {
            eprintln!("{color} ne s'est pas connecté");
            return Ok(None);
        };
        let mut conn = LineConn::new(stream).context("socket")?;
        eprintln!("CONNECTED {color}");
        let _ = conn.send(ServerMsg::NewGame {
            rows: SIZE.into(),
            cols: SIZE.into(),
            time_ms,
            color,
        });
        if color == Color::Black {
            let _ = conn.send(ServerMsg::Board {
                color,
                board: board.clone(),
            });
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

fn accept(listener: &TcpListener, deadline: Instant) -> std::io::Result<Option<TcpStream>> {
    listener.set_nonblocking(true)?;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                return Ok(Some(stream));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
}

struct Verdict {
    winner: Color,
    reason: Reason,
}

struct Referee {
    board: Board,
    players: ByColor<LineConn>,
    time: Duration,
    max_invalid: u32,
    moves: Vec<Move>,
    move_ms: Vec<u64>,
    invalid: InvalidCounts,
}

impl Referee {
    fn send(&mut self, color: Color, msg: ServerMsg) -> bool {
        self.players[color].send(msg).is_ok()
    }

    fn run(&mut self) -> Verdict {
        let mut turn = Color::Red;
        loop {
            let request = match self.moves.last() {
                None => ServerMsg::Board {
                    color: Color::Red,
                    board: self.board.clone(),
                },
                Some(&last) => ServerMsg::Request(Some(last)),
            };
            // Temps compté contre le joueur : réseau et coups refusés compris.
            let asked = Instant::now();
            if !self.send(turn, request) {
                return self.forfeit(turn, Reason::Disconnect);
            }
            let mv = match self.receive_move(turn) {
                Ok(mv) => mv,
                Err(reason) => return self.forfeit(turn, reason),
            };
            self.board.apply(mv);
            self.moves.push(mv);
            let ms = millis(asked);
            self.move_ms.push(ms);
            announce(&RefereeEvent::Move { mv: mv.to_string(), ms });
            if let Some(winner) = rules::winner_after(&self.board, turn) {
                for (color, outcome) in [(winner, Outcome::Win), (winner.other(), Outcome::Loss)] {
                    self.send(
                        color,
                        ServerMsg::End {
                            outcome,
                            last: Some(mv),
                        },
                    );
                }
                return Verdict {
                    winner,
                    reason: Reason::NoMoves,
                };
            }
            turn = turn.other();
        }
    }

    /// Premier coup légal de `turn` avant l'échéance ; chaque coup refusé est répondu par `4`.
    fn receive_move(&mut self, turn: Color) -> Result<Move, Reason> {
        let deadline = Instant::now() + self.time;
        loop {
            let line = match self.players[turn].recv(Some(deadline)) {
                Ok(line) => line,
                Err(RecvError::Timeout) => return Err(Reason::Timeout),
                Err(_) => return Err(Reason::Disconnect),
            };
            match line.parse() {
                Ok(mv) if rules::is_legal(&self.board, turn, mv) => return Ok(mv),
                _ => {
                    self.invalid[turn] += 1;
                    eprintln!("coup invalide de {turn} : {line:?}");
                    if self.invalid[turn] >= self.max_invalid {
                        return Err(Reason::Protocol);
                    }
                    if !self.send(turn, ServerMsg::Invalid) {
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
            self.send(color, ServerMsg::End { outcome, last: None });
        }
        Verdict { winner, reason }
    }
}
