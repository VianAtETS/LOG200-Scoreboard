//! Protocole texte du serveur officiel, une commande par ligne (tableau dans docs/formats.md).

use std::fmt;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpStream};
use std::str::FromStr;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::board::{Board, BoardError, Color, Move};

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Win,
    Loss,
    /// Envoyé aux deux joueurs quand l'un d'eux dépasse son temps.
    Timeout,
}

impl Outcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Win => "WIN",
            Self::Loss => "LOSS",
            Self::Timeout => "TIMEOUT",
        }
    }
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Outcome {
    type Err = DecodeError;

    fn from_str(s: &str) -> Result<Self, DecodeError> {
        [Self::Win, Self::Loss, Self::Timeout]
            .into_iter()
            .find(|o| o.as_str() == s)
            .ok_or_else(|| DecodeError::UnknownOutcome(s.to_string()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerMsg {
    NewGame {
        rows: u32,
        cols: u32,
        time_ms: u64,
        color: Color,
    },
    /// Pour le rouge, ce message vaut aussi demande de coup.
    Board {
        color: Color,
        board: Board,
    },
    /// `None` si le coup adverse est absent ou illisible (anciens serveurs : `3 A8-A8`).
    Request(Option<Move>),
    Invalid,
    End {
        outcome: Outcome,
        last: Option<Move>,
    },
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("commande inconnue : {0:?}")]
    UnknownCommand(String),
    #[error("message mal formé : {0:?}")]
    Malformed(String),
    #[error("couleur inconnue : {0}")]
    UnknownColor(u64),
    #[error("résultat inconnu : {0:?}")]
    UnknownOutcome(String),
    #[error(transparent)]
    Board(#[from] BoardError),
}

const fn color_code(color: Color) -> u8 {
    match color {
        Color::Red => 0,
        Color::Black => 1,
    }
}

fn numbers<T: FromStr>(line: &str, fields: &str) -> Result<Vec<T>, DecodeError> {
    fields
        .split_whitespace()
        .map(|t| t.parse().map_err(|_| DecodeError::Malformed(line.to_string())))
        .collect()
}

impl fmt::Display for ServerMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::NewGame {
                rows,
                cols,
                time_ms,
                color,
            } => write!(f, "0 {PROTOCOL_VERSION} {rows} {cols} {time_ms} {}", color_code(*color)),
            Self::Board { color, board } => {
                write!(f, "{}", color_code(*color) + 1)?;
                board.cells().iter().try_for_each(|cell| write!(f, " {}", cell.code()))
            }
            Self::Request(Some(mv)) => write!(f, "3 {}", mv.protocol()),
            Self::Request(None) => f.write_str("3"),
            Self::Invalid => f.write_str("4"),
            Self::End {
                outcome,
                last: Some(mv),
            } => write!(f, "5 {outcome} {}", mv.protocol()),
            Self::End { outcome, last: None } => write!(f, "5 {outcome}"),
        }
    }
}

impl FromStr for ServerMsg {
    type Err = DecodeError;

    fn from_str(line: &str) -> Result<Self, DecodeError> {
        let line = line.trim();
        let (code, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match code {
            "0" => {
                let [_version, rows, cols, time_ms, color] = numbers::<u64>(line, rest)?[..] else {
                    return Err(DecodeError::Malformed(line.to_string()));
                };
                let narrow = |n| u32::try_from(n).map_err(|_| DecodeError::Malformed(line.to_string()));
                Ok(Self::NewGame {
                    rows: narrow(rows)?,
                    cols: narrow(cols)?,
                    time_ms,
                    color: match color {
                        0 => Color::Red,
                        1 => Color::Black,
                        other => return Err(DecodeError::UnknownColor(other)),
                    },
                })
            }
            "1" | "2" => Ok(Self::Board {
                color: if code == "1" { Color::Red } else { Color::Black },
                board: Board::from_cells(&numbers::<u8>(line, rest)?)?,
            }),
            "3" => Ok(Self::Request(rest.parse().ok())),
            "4" => Ok(Self::Invalid),
            "5" => {
                let (outcome, last) = rest.split_once(' ').unwrap_or((rest, ""));
                Ok(Self::End {
                    outcome: outcome.parse()?,
                    last: last.parse().ok(),
                })
            }
            _ => Err(DecodeError::UnknownCommand(line.to_string())),
        }
    }
}

#[derive(Debug, Error)]
pub enum RecvError {
    #[error("délai dépassé")]
    Timeout,
    #[error("connexion fermée")]
    Closed,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Connexion ligne par ligne avec échéance. Les octets d'une ligne incomplète sont conservés d'un
/// appel à l'autre.
pub struct LineConn {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    pending: Vec<u8>,
}

impl LineConn {
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        Ok(Self {
            writer: stream.try_clone()?,
            reader: BufReader::new(stream),
            pending: Vec::new(),
        })
    }

    pub fn send(&mut self, line: impl fmt::Display) -> io::Result<()> {
        self.writer.write_all(format!("{line}\n").as_bytes())?;
        self.writer.flush()
    }

    /// Ligne suivante, sans fin de ligne. Sans échéance, l'attente est illimitée.
    pub fn recv(&mut self, deadline: Option<Instant>) -> Result<String, RecvError> {
        loop {
            let timeout = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(RecvError::Timeout);
                    }
                    Some(left.max(Duration::from_millis(1)))
                }
                None => None,
            };
            self.reader.get_ref().set_read_timeout(timeout)?;
            match self.reader.read_until(b'\n', &mut self.pending) {
                Ok(_) if self.pending.ends_with(b"\n") => {
                    let line = String::from_utf8_lossy(&self.pending)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    self.pending.clear();
                    return Ok(line);
                }
                // Fin de flux, avec ou sans ligne incomplète.
                Ok(_) => return Err(RecvError::Closed),
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) => {}
                Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted) => {
                    return Err(RecvError::Closed);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    pub fn shutdown(&self) {
        let _ = self.writer.shutdown(Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    fn mv(s: &str) -> Move {
        s.parse().unwrap()
    }

    #[test]
    fn encode_matches_server_strings() {
        let m = mv("G1-E3-C12");
        let new = ServerMsg::NewGame {
            rows: 12,
            cols: 12,
            time_ms: 5000,
            color: Color::Red,
        };
        assert_eq!(new.to_string(), "0 2 12 12 5000 0");
        assert_eq!(ServerMsg::Request(Some(m)).to_string(), "3 G1 - E3 - C12");
        let end = ServerMsg::End {
            outcome: Outcome::Win,
            last: Some(m),
        };
        assert_eq!(end.to_string(), "5 WIN G1 - E3 - C12");
        let timeout = ServerMsg::End {
            outcome: Outcome::Timeout,
            last: None,
        };
        assert_eq!(timeout.to_string(), "5 TIMEOUT");
        assert_eq!(ServerMsg::Request(None).to_string(), "3");
        assert_eq!(ServerMsg::Invalid.to_string(), "4");
        let board = ServerMsg::Board {
            color: Color::Black,
            board: Board::initial(),
        };
        assert!(board.to_string().starts_with("2 0 0 0 0 0 3 0"));
    }

    #[test]
    fn decode_roundtrip_and_legacy_first_request() {
        for msg in [
            ServerMsg::NewGame {
                rows: 12,
                cols: 12,
                time_ms: 500,
                color: Color::Black,
            },
            ServerMsg::Board {
                color: Color::Red,
                board: Board::initial(),
            },
            ServerMsg::Request(Some(mv("A1-B2-C3"))),
            ServerMsg::Invalid,
            ServerMsg::End {
                outcome: Outcome::Loss,
                last: Some(mv("L12-K11-J10")),
            },
            ServerMsg::End {
                outcome: Outcome::Timeout,
                last: None,
            },
        ] {
            assert_eq!(msg.to_string().parse::<ServerMsg>().unwrap(), msg);
        }
        assert_eq!("3 A8-A8".parse::<ServerMsg>().unwrap(), ServerMsg::Request(None));
    }

    #[test]
    fn decode_rejects_out_of_range_values() {
        let mut cells = ServerMsg::Board {
            color: Color::Red,
            board: Board::initial(),
        }
        .to_string();
        cells.replace_range(2..3, "256");
        assert!(matches!(cells.parse::<ServerMsg>(), Err(DecodeError::Malformed(_))));
        assert!(matches!(
            "0 2 12 12 500 7".parse::<ServerMsg>(),
            Err(DecodeError::UnknownColor(7))
        ));
        assert!(matches!(
            "5 DRAW".parse::<ServerMsg>(),
            Err(DecodeError::UnknownOutcome(_))
        ));
        assert!(matches!("9".parse::<ServerMsg>(), Err(DecodeError::UnknownCommand(_))));
    }

    #[test]
    fn recv_keeps_partial_line_across_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(b"G1-").unwrap();
            std::thread::sleep(Duration::from_millis(80));
            s.write_all(b"E3-C12\r\nnext\n").unwrap();
            std::thread::sleep(Duration::from_millis(50));
        });
        let (stream, _) = listener.accept().unwrap();
        let mut conn = LineConn::new(stream).unwrap();
        let short = Instant::now() + Duration::from_millis(20);
        assert!(matches!(conn.recv(Some(short)), Err(RecvError::Timeout)));
        let long = Instant::now() + Duration::from_secs(2);
        assert_eq!(conn.recv(Some(long)).unwrap(), "G1-E3-C12");
        assert_eq!(conn.recv(Some(long)).unwrap(), "next");
        assert!(matches!(conn.recv(Some(long)), Err(RecvError::Closed)));
        writer.join().unwrap();
    }
}
