//! Messages du serveur officiel, décodés pour Isolation (plateau 12×12, coups à trois cases). Le
//! cadre commun (connexion, encodage, résultats) est dans `game_kit::protocol`.

use std::fmt;
use std::str::FromStr;

use game_kit::protocol as wire;
use thiserror::Error;

pub use game_kit::protocol::{LineConn, Outcome, PROTOCOL_VERSION, RecvError};

use crate::board::{Board, BoardError, Color, Move};

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

fn numbers<T: FromStr>(line: &str, fields: &str) -> Result<Vec<T>, DecodeError> {
    fields
        .split_whitespace()
        .map(|t| t.parse().map_err(|_| DecodeError::Malformed(line.to_string())))
        .collect()
}

impl fmt::Display for ServerMsg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let protocol = |mv: &Option<Move>| mv.map(|mv| mv.protocol().to_string());
        let line = match self {
            Self::NewGame {
                rows,
                cols,
                time_ms,
                color,
            } => wire::new_game(*rows, *cols, *time_ms, *color),
            Self::Board { color, board } => {
                let cells: Vec<u8> = board.cells().iter().map(|cell| cell.code()).collect();
                wire::board(*color, &cells)
            }
            Self::Request(mv) => wire::request(protocol(mv).as_deref()),
            Self::Invalid => wire::INVALID.to_string(),
            Self::End { outcome, last } => wire::end(*outcome, protocol(last).as_deref()),
        };
        f.write_str(&line)
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
                    outcome: outcome
                        .parse()
                        .map_err(|e: wire::UnknownOutcome| DecodeError::UnknownOutcome(e.0))?,
                    last: last.parse().ok(),
                })
            }
            _ => Err(DecodeError::UnknownCommand(line.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
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
            "5 TIE".parse::<ServerMsg>(),
            Err(DecodeError::UnknownOutcome(_))
        ));
        assert!(matches!("9".parse::<ServerMsg>(), Err(DecodeError::UnknownCommand(_))));
    }
}
