//! Notation des cases et des coups. Le parseur a la tolérance du serveur officiel : ' ' et '-'
//! séparent en nombre quelconque, y compris zéro (`G1E3C12`), et la casse est ignorée.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::board::{Move, SIZE, Square};

#[derive(Debug, Error, PartialEq, Eq)]
#[error("notation illisible : {0:?}")]
pub struct NotationError(pub String);

impl FromStr for Square {
    type Err = NotationError;

    fn from_str(s: &str) -> Result<Self, NotationError> {
        let mut parser = Parser::new(s.trim());
        parser
            .square()
            .filter(|_| parser.finish())
            .ok_or_else(|| NotationError(s.to_string()))
    }
}

impl FromStr for Move {
    type Err = NotationError;

    /// Exige exactement trois cases, sans vérifier la légalité.
    fn from_str(s: &str) -> Result<Self, NotationError> {
        let mut parser = Parser::new(s.trim());
        parser
            .mv()
            .filter(|_| parser.finish())
            .ok_or_else(|| NotationError(s.to_string()))
    }
}

impl Move {
    /// Format des messages 3 et 5 du serveur officiel : `G1 - E3 - C12`.
    pub const fn protocol(self) -> impl fmt::Display {
        ProtocolMove(self)
    }
}

/// Coups séparés par des virgules, comme l'option `--opening` de l'arbitre.
pub fn join(moves: &[Move]) -> String {
    moves.iter().map(Move::to_string).collect::<Vec<_>>().join(",")
}

struct ProtocolMove(Move);

impl fmt::Display for ProtocolMove {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Move { from, to, destroyed } = self.0;
        write!(f, "{from} - {to} - {destroyed}")
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    const fn new(s: &'a str) -> Self {
        Self {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    /// Vrai s'il ne reste que des séparateurs.
    fn finish(&mut self) -> bool {
        self.skip_separators();
        self.pos == self.bytes.len()
    }

    fn skip_separators(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'-')) {
            self.pos += 1;
        }
    }

    fn mv(&mut self) -> Option<Move> {
        Some(Move {
            from: self.next_square()?,
            to: self.next_square()?,
            destroyed: self.next_square()?,
        })
    }

    fn next_square(&mut self) -> Option<Square> {
        self.skip_separators();
        self.square()
    }

    fn square(&mut self) -> Option<Square> {
        let x = self.bytes.get(self.pos)?.to_ascii_uppercase().checked_sub(b'A')?;
        self.pos += 1;
        let digits = self.bytes[self.pos..].iter().take_while(|b| b.is_ascii_digit()).count();
        let row: u8 = std::str::from_utf8(&self.bytes[self.pos..self.pos + digits])
            .ok()?
            .parse()
            .ok()?;
        self.pos += digits;
        Square::new(x, SIZE.checked_sub(row)?).filter(|_| row > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical(s: &str) -> String {
        s.parse::<Move>().map(|m| m.to_string()).unwrap_or_default()
    }

    #[test]
    fn accepts_all_server_formats() {
        for s in ["G1-E3-C12", "G1E3C12", "G1 - E3 - C12", "g1 e3 c12", "  G1--E3 C12 "] {
            assert_eq!(canonical(s), "G1-E3-C12", "{s:?}");
        }
    }

    #[test]
    fn rejects_malformed() {
        for s in [
            "",
            "G1-E3",
            "G1-E3-C12-D4",
            "G0-E3-C12",
            "G13-E3-C12",
            "G999-E3-C12",
            "M1-E3-C12",
            "G1;E3;C12",
            "G-E3-C12",
            "@1-E3-C12",
        ] {
            assert!(s.parse::<Move>().is_err(), "{s:?}");
        }
    }

    #[test]
    fn coordinates_follow_server_convention() {
        let mv: Move = "A12-L1-F12".parse().unwrap();
        assert_eq!(mv.from, Square::new(0, 0).unwrap());
        assert_eq!(mv.to, Square::new(11, 11).unwrap());
        assert_eq!(mv.destroyed, Square::new(5, 0).unwrap());
        assert_eq!(mv.protocol().to_string(), "A12 - L1 - F12");
        assert_eq!("l1 ".parse(), Ok(Square::new(11, 11).unwrap()));
    }
}
