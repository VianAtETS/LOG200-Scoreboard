//! Socle commun des connexions WebSocket du ladder (`/api/remote`, `/api/live`) : accueil des connexions
//! avec plafond, refus de poignée de main, fermeture propre.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::handshake::server::ErrorResponse;
use tungstenite::http::StatusCode;
use tungstenite::protocol::CloseFrame;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Error as WsError, WebSocket};

const ACCEPT_RETRY: Duration = Duration::from_millis(100);
const MAX_REASON: usize = 123;

pub fn listen(listener: TcpListener, max: usize, handle: impl Fn(TcpStream) + Clone + Send + 'static) {
    let open = Arc::new(AtomicUsize::new(0));
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else {
                thread::sleep(ACCEPT_RETRY);
                continue;
            };
            let counted = Counted::enter(&open);
            if counted.0.load(Ordering::Relaxed) > max {
                continue;
            }
            let handle = handle.clone();
            let _ = thread::Builder::new().spawn(move || {
                let _counted = counted;
                handle(stream);
            });
        }
    });
}

struct Counted(Arc<AtomicUsize>);

impl Counted {
    fn enter(open: &Arc<AtomicUsize>) -> Self {
        open.fetch_add(1, Ordering::Relaxed);
        Self(Arc::clone(open))
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn refusal(status: StatusCode, body: &str) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(format!("{body}\n")));
    *response.status_mut() = status;
    response
}

pub fn goodbye(ws: &mut WebSocket<TcpStream>, code: CloseCode, reason: &str) {
    let mut end = reason.len().min(MAX_REASON);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let frame = CloseFrame {
        code,
        reason: reason[..end].into(),
    };
    if ws.close(Some(frame)).is_err() {
        return;
    }
    let _ = ws.get_ref().set_nonblocking(false);
    let _ = ws.get_ref().set_read_timeout(Some(Duration::from_millis(100)));
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        match ws.read() {
            Ok(_) => {}
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
}
