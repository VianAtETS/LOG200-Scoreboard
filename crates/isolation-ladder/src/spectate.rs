//! Spectateurs de `/api/live` : l'état des parties en cours, puis chaque changement (protocole dans
//! docs/formats.md). Le client n'a rien à dire : une trame de données de sa part ferme le flux.

use std::io::ErrorKind;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use anyhow::Context;
use tungstenite::handshake::server::{Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::{Bytes, Error as WsError, Message as Frame, WebSocket};

use crate::config::Config;
use crate::hub::{Hub, Subscription};
use crate::ws::{goodbye, listen, refusal};

pub const PATH: &str = "/api/live";
/// Séparé du plafond de `/api/remote` : une salle pleine ne bloque pas les équipes.
const MAX_SPECTATORS: usize = 512;
const MAX_MESSAGE: usize = 1 << 10;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Un client qui ne lit plus libère son thread en au plus ce délai.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Cloudflare coupe une WebSocket muette après 100 s.
const PING_EVERY: Duration = Duration::from_secs(10);
const SILENCE_LIMIT: Duration = Duration::from_secs(30);
/// Au-delà, le client se reconnecte : une connexion ne garde pas sa place indéfiniment.
const MAX_LIFETIME: Duration = Duration::from_hours(2);

/// Écoute `/api/live` dans un thread, si `[live]` est configuré.
pub fn serve(hub: &Arc<Hub>, cfg: &Config) -> anyhow::Result<()> {
    let Some(live) = &cfg.live else {
        return Ok(());
    };
    let listener = TcpListener::bind(&live.listen).with_context(|| format!("écoute du direct sur {}", live.listen))?;
    eprintln!("direct : ws://{}{PATH}", live.listen);
    watch_on(listener, Arc::clone(hub));
    Ok(())
}

fn watch_on(listener: TcpListener, hub: Arc<Hub>) {
    listen(listener, MAX_SPECTATORS, move |stream| watch(stream, &hub));
}

fn watch(stream: TcpStream, hub: &Arc<Hub>) {
    let Ok(mut ws) = accept(stream) else {
        return;
    };
    let ending = match hub.subscribe() {
        Some(subscription) => relay(&mut ws, &subscription),
        None => Some((CloseCode::Away, "arrêt du ladder")),
    };
    if let Some((code, reason)) = ending {
        goodbye(&mut ws, code, reason);
    }
}

fn accept(stream: TcpStream) -> Result<WebSocket<TcpStream>, WsError> {
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
    let _ = stream.set_nodelay(true);
    #[expect(clippy::result_large_err, reason = "type d'erreur imposé par tungstenite")]
    let check = |request: &Request, response: Response| {
        if request.uri().path() == PATH {
            Ok(response)
        } else {
            Err(refusal(StatusCode::NOT_FOUND, &format!("seul {PATH} est servi ici")))
        }
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    tungstenite::accept_hdr_with_config(stream, check, Some(config)).map_err(|e| match e {
        tungstenite::HandshakeError::Failure(e) => e,
        tungstenite::HandshakeError::Interrupted(_) => WsError::ConnectionClosed,
    })
}

fn relay(ws: &mut WebSocket<TcpStream>, subscription: &Subscription) -> Option<(CloseCode, &'static str)> {
    let since = Instant::now();
    let mut heard = since;
    ws.send(Frame::text(subscription.snapshot.to_string())).ok()?;
    loop {
        let frame = match subscription.events.recv_timeout(PING_EVERY) {
            Ok(message) => Frame::text(message.to_string()),
            Err(RecvTimeoutError::Timeout) => Frame::Ping(Bytes::new()),
            Err(RecvTimeoutError::Disconnected) => return Some((CloseCode::Away, "flux interrompu")),
        };
        ws.send(frame).ok()?;
        ws.get_ref().set_nonblocking(true).ok()?;
        loop {
            match ws.read() {
                Ok(Frame::Pong(_) | Frame::Ping(_)) => heard = Instant::now(),
                Ok(Frame::Text(_) | Frame::Binary(_)) => {
                    return Some((CloseCode::Policy, "ce flux n'accepte aucun message"));
                }
                Ok(Frame::Frame(_)) => {}
                Err(WsError::Io(e)) if e.kind() == ErrorKind::WouldBlock => break,
                Ok(Frame::Close(_)) | Err(_) => return None,
            }
        }
        ws.get_ref().set_nonblocking(false).ok()?;
        if heard.elapsed() > SILENCE_LIMIT {
            return None;
        }
        if since.elapsed() > MAX_LIFETIME {
            return Some((CloseCode::Normal, "durée maximale atteinte : se reconnecter"));
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tungstenite::client::IntoClientRequest;

    use isolation_core::record::RefereeEvent;

    use super::*;
    use crate::hub;

    fn fixture() -> (Arc<Hub>, String) {
        let hub = Hub::new(|_, _, _| {});
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        watch_on(listener, Arc::clone(&hub));
        (hub, addr)
    }

    fn start(hub: &Hub) {
        hub.start(&hub::Start {
            id: "g1",
            red: "a",
            black: "b",
            since: "",
            time_ms: 500,
            opening: &[],
            tournament: None,
        });
    }

    fn connect(addr: &str, path: &str) -> Result<WebSocket<TcpStream>, WsError> {
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let request = format!("ws://{addr}{path}").into_client_request().unwrap();
        tungstenite::client(request, stream)
            .map(|(ws, _)| ws)
            .map_err(|e| match e {
                tungstenite::HandshakeError::Failure(e) => e,
                tungstenite::HandshakeError::Interrupted(_) => unreachable!("flux bloquant"),
            })
    }

    fn recv(ws: &mut WebSocket<TcpStream>) -> Result<Value, Option<CloseCode>> {
        loop {
            match ws.read() {
                Ok(Frame::Text(text)) => return Ok(serde_json::from_str(text.as_str()).unwrap()),
                Ok(Frame::Close(frame)) => return Err(frame.map(|f| f.code)),
                Ok(_) => {}
                Err(_) => return Err(None),
            }
        }
    }

    #[test]
    fn another_path_is_refused_before_the_upgrade() {
        let (_hub, addr) = fixture();
        match connect(&addr, "/api/remote") {
            Err(WsError::Http(response)) => assert_eq!(response.status(), StatusCode::NOT_FOUND),
            other => panic!("refus attendu, pas {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn a_spectator_gets_the_snapshot_then_the_moves() {
        let (hub, addr) = fixture();
        start(&hub);
        let mut ws = connect(&addr, PATH).unwrap();
        let snapshot = recv(&mut ws).unwrap();
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["games"][0]["id"], "g1");
        let mv = RefereeEvent::Move {
            mv: "G1-G4-F9".into(),
            ms: 7,
        };
        hub.event("g1", mv);
        let moved = recv(&mut ws).unwrap();
        assert_eq!((moved["type"].as_str(), moved["n"].as_u64()), (Some("move"), Some(0)));
    }

    #[test]
    fn a_message_from_the_spectator_closes_the_stream() {
        let (hub, addr) = fixture();
        let mut ws = connect(&addr, PATH).unwrap();
        recv(&mut ws).unwrap();
        ws.send(Frame::text("bonjour")).unwrap();
        start(&hub);
        assert_eq!(recv(&mut ws).unwrap()["type"], "started");
        assert_eq!(recv(&mut ws), Err(Some(CloseCode::Policy)));
    }

    #[test]
    fn shutdown_says_goodbye() {
        let (hub, addr) = fixture();
        let mut ws = connect(&addr, PATH).unwrap();
        recv(&mut ws).unwrap();
        hub.shutdown();
        assert_eq!(recv(&mut ws), Err(Some(CloseCode::Away)));
    }
}
