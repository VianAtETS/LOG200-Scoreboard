//! Spectateurs de `/api/live` : l'état des parties en cours, puis chaque changement (protocole dans
//! docs/formats.md). Le client n'a rien à dire : une trame de données de sa part ferme le flux.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade, close_code};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::time::{Instant, MissedTickBehavior, interval_at, timeout};

use crate::hub::{Hub, Subscription};

pub const PATH: &str = "/api/live";
/// Séparé du plafond de `/api/remote` : une salle pleine ne bloque pas les équipes.
const MAX_SPECTATORS: usize = 512;
const MAX_MESSAGE: usize = 1 << 10;
/// Un client qui ne lit plus libère sa place en au plus ce délai.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Cloudflare coupe une WebSocket muette après 100 s.
const PING_EVERY: Duration = Duration::from_secs(10);
const SILENCE_LIMIT: Duration = Duration::from_secs(30);
/// Au-delà, le client se reconnecte : une connexion ne garde pas sa place indéfiniment.
const MAX_LIFETIME: Duration = Duration::from_hours(2);

pub async fn watch(State(hub): State<Arc<Hub>>, upgrade: WebSocketUpgrade) -> Response {
    if hub.watching() >= MAX_SPECTATORS {
        return (StatusCode::SERVICE_UNAVAILABLE, "salle pleine : réessayer plus tard\n").into_response();
    }
    upgrade
        .max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |mut socket| async move {
            let ending = match hub.subscribe() {
                Some(subscription) => relay(&mut socket, subscription).await,
                None => Some((close_code::AWAY, "arrêt du ladder")),
            };
            if let Some((code, reason)) = ending {
                let frame = CloseFrame {
                    code,
                    reason: reason.into(),
                };
                send(&mut socket, Message::Close(Some(frame))).await;
            }
        })
}

/// Pinged on a clock of its own: a busy stream sends no ping, and its pongs would stop.
async fn relay(socket: &mut WebSocket, mut subscription: Subscription) -> Option<(u16, &'static str)> {
    let since = Instant::now();
    let mut heard = since;
    send(socket, Message::Text(subscription.snapshot.as_ref().into())).await?;
    let mut ping = interval_at(since + PING_EVERY, PING_EVERY);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            event = subscription.events.recv() => match event {
                Some(message) => send(socket, Message::Text(message.as_ref().into())).await?,
                None => return Some((close_code::AWAY, "flux interrompu")),
            },
            _ = ping.tick() => {
                if heard.elapsed() > SILENCE_LIMIT {
                    return None;
                }
                if since.elapsed() > MAX_LIFETIME {
                    return Some((close_code::NORMAL, "durée maximale atteinte : se reconnecter"));
                }
                send(socket, Message::Ping(Bytes::new())).await?;
            }
            frame = socket.recv() => match frame {
                Some(Ok(Message::Pong(_) | Message::Ping(_))) => heard = Instant::now(),
                Some(Ok(Message::Text(_) | Message::Binary(_))) => {
                    return Some((close_code::POLICY, "ce flux n'accepte aucun message"));
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return None,
            },
        }
    }
}

async fn send(socket: &mut WebSocket, message: Message) -> Option<()> {
    timeout(WRITE_TIMEOUT, socket.send(message)).await.ok()?.ok()
}

#[cfg(test)]
mod tests {
    use std::net::TcpStream;

    use axum::Router;
    use axum::routing::get;
    use serde_json::Value;
    use tokio::runtime::Runtime;
    use tungstenite::protocol::frame::coding::CloseCode;
    use tungstenite::{Message as Frame, WebSocket as Client};

    use isolation_core::record::RefereeEvent;

    use super::*;
    use crate::hub;

    struct Fixture {
        hub: Arc<Hub>,
        addr: String,
        _runtime: Runtime,
    }

    fn fixture() -> Fixture {
        let hub = Hub::new(|_, _, _| {});
        let runtime = Runtime::new().unwrap();
        let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let router = Router::new().route(PATH, get(watch)).with_state(Arc::clone(&hub));
        runtime.spawn(async move { axum::serve(listener, router).await });
        Fixture {
            hub,
            addr,
            _runtime: runtime,
        }
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

    fn connect(addr: &str) -> Client<tungstenite::stream::MaybeTlsStream<TcpStream>> {
        tungstenite::connect(format!("ws://{addr}{PATH}")).unwrap().0
    }

    fn recv(ws: &mut Client<tungstenite::stream::MaybeTlsStream<TcpStream>>) -> Result<Value, Option<CloseCode>> {
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
    fn a_spectator_gets_the_snapshot_then_the_moves() {
        let live = fixture();
        start(&live.hub);
        let mut ws = connect(&live.addr);
        let snapshot = recv(&mut ws).unwrap();
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["games"][0]["id"], "g1");
        let mv = RefereeEvent::Move {
            mv: "G1-G4-F9".into(),
            ms: 7,
        };
        live.hub.event("g1", mv);
        let moved = recv(&mut ws).unwrap();
        assert_eq!((moved["type"].as_str(), moved["n"].as_u64()), (Some("move"), Some(0)));
    }

    #[test]
    fn a_message_from_the_spectator_closes_the_stream() {
        let live = fixture();
        let mut ws = connect(&live.addr);
        recv(&mut ws).unwrap();
        ws.send(Frame::text("bonjour")).unwrap();
        assert_eq!(recv(&mut ws), Err(Some(CloseCode::Policy)));
    }

    #[test]
    fn shutdown_says_goodbye() {
        let live = fixture();
        let mut ws = connect(&live.addr);
        recv(&mut ws).unwrap();
        live.hub.shutdown();
        assert_eq!(recv(&mut ws), Err(Some(CloseCode::Away)));
    }
}
