//! Cadre du protocole texte du serveur officiel, commun à ses jeux : une commande par ligne
//! (tableau dans docs/formats.md). Le contenu du plateau et la notation des coups sont propres à
//! chaque jeu ; les messages sont encodés ici pour que tous les arbitres écrivent les mêmes.

use std::fmt;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpStream};
use std::str::FromStr;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::side::Color;

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

#[derive(Debug, Error, PartialEq, Eq)]
#[error("résultat inconnu : {0:?}")]
pub struct UnknownOutcome(pub String);

impl FromStr for Outcome {
    type Err = UnknownOutcome;

    fn from_str(s: &str) -> Result<Self, UnknownOutcome> {
        [Self::Win, Self::Loss, Self::Timeout]
            .into_iter()
            .find(|o| o.as_str() == s)
            .ok_or_else(|| UnknownOutcome(s.to_string()))
    }
}

/// `0 2 <lignes> <colonnes> <temps> <couleur>` : début de partie.
pub fn new_game(rows: u32, cols: u32, time_ms: u64, color: Color) -> String {
    format!("0 {PROTOCOL_VERSION} {rows} {cols} {time_ms} {}", color.code())
}

/// `1 <cases>` au rouge, `2 <cases>` au noir. Pour le rouge, ce message vaut demande de coup.
pub fn board(color: Color, cells: &[u8]) -> String {
    let mut line = (color.code() + 1).to_string();
    for cell in cells {
        line.push(' ');
        line.push_str(&cell.to_string());
    }
    line
}

/// `3 <coup adverse>` : demande de coup.
pub fn request(last: Option<&str>) -> String {
    last.map_or_else(|| "3".to_string(), |mv| format!("3 {mv}"))
}

/// Coup refusé.
pub const INVALID: &str = "4";

/// `5 <résultat> [<dernier coup>]` : fin de partie.
pub fn end(outcome: Outcome, last: Option<&str>) -> String {
    last.map_or_else(|| format!("5 {outcome}"), |mv| format!("5 {outcome} {mv}"))
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

    #[test]
    fn messages_match_server_strings() {
        assert_eq!(new_game(12, 12, 5000, Color::Red), "0 2 12 12 5000 0");
        assert_eq!(board(Color::Black, &[0, 3, 5]), "2 0 3 5");
        assert_eq!(request(Some("G1 - E3 - C12")), "3 G1 - E3 - C12");
        assert_eq!(request(None), "3");
        assert_eq!(end(Outcome::Win, Some("G1 - E3 - C12")), "5 WIN G1 - E3 - C12");
        assert_eq!(end(Outcome::Timeout, None), "5 TIMEOUT");
        assert_eq!("LOSS".parse(), Ok(Outcome::Loss));
        assert!("DRAW".parse::<Outcome>().is_err());
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
