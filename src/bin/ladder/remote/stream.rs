//! Connexion du relais à l'arbitre, à la place du jar : la socket Unix de sa couleur dans le bac à
//! sable de l'arbitre, ou TCP quand l'arbitre tourne sans bac à sable.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

pub enum RefereeStream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(UnixStream),
}

impl RefereeStream {
    /// `unix:<chemin>` ou `tcp:<hôte>:<port>`, gabarit déjà substitué.
    pub fn connect(target: &str) -> io::Result<Self> {
        if let Some(addr) = target.strip_prefix("tcp:") {
            let stream = TcpStream::connect(addr)?;
            stream.set_nodelay(true)?;
            return Ok(Self::Tcp(stream));
        }
        if let Some(path) = target.strip_prefix("unix:") {
            #[cfg(unix)]
            return UnixStream::connect(path).map(Self::Unix);
            #[cfg(not(unix))]
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("socket Unix {path}"),
            ));
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("cible inconnue : {target}"),
        ))
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        match self {
            Self::Tcp(s) => s.try_clone().map(Self::Tcp),
            #[cfg(unix)]
            Self::Unix(s) => s.try_clone().map(Self::Unix),
        }
    }

    /// Débloque aussi la lecture en cours sur un clone.
    pub fn shutdown(&self) {
        let _ = match self {
            Self::Tcp(s) => s.shutdown(Shutdown::Both),
            #[cfg(unix)]
            Self::Unix(s) => s.shutdown(Shutdown::Both),
        };
    }
}

impl Read for RefereeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.read(buf),
        }
    }
}

impl Write for RefereeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            #[cfg(unix)]
            Self::Unix(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            #[cfg(unix)]
            Self::Unix(s) => s.flush(),
        }
    }
}
