//! Une WebSocket axum servie par une tâche async (`pump`), lue et écrite par un thread ordinaire
//! (`Socket`) : les équipes distantes et les nœuds vivent dans des threads, comme les parties.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message as Frame, WebSocket, close_code};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::timeout;

/// Limite de la raison d'une trame de fermeture (125 octets moins le code).
const MAX_REASON: usize = 123;
const CLOSE_WAIT: Duration = Duration::from_secs(1);

/// L'autre bout est parti.
pub struct Gone;

/// The session thread's end of the client's WebSocket, which an async pump serves.
pub struct Socket {
    incoming: Receiver<Frame>,
    outgoing: UnboundedSender<Frame>,
}

impl Socket {
    pub fn read(&self, wait: Duration) -> Result<Option<Frame>, Gone> {
        match self.incoming.recv_timeout(wait) {
            Ok(frame) => Ok(Some(frame)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Gone),
        }
    }

    pub fn send(&self, frame: Frame) -> Result<(), Gone> {
        self.outgoing.send(frame).map_err(|_| Gone)
    }

    pub fn goodbye(&self, reason: &str) {
        let mut end = reason.len().min(MAX_REASON);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        let frame = CloseFrame {
            code: close_code::POLICY,
            reason: reason[..end].into(),
        };
        let _ = self.send(Frame::Close(Some(frame)));
    }
}

pub fn connect(socket: WebSocket) -> (Socket, impl Future<Output = ()>) {
    let (incoming_tx, incoming) = mpsc::channel();
    let (outgoing, outgoing_rx) = unbounded_channel();
    (Socket { incoming, outgoing }, pump(socket, incoming_tx, outgoing_rx))
}

/// Lives until the client or the session leaves; after a close frame, waits briefly for the client's.
async fn pump(mut socket: WebSocket, incoming: Sender<Frame>, mut outgoing: UnboundedReceiver<Frame>) {
    loop {
        tokio::select! {
            frame = outgoing.recv() => match frame {
                Some(frame @ Frame::Close(_)) => {
                    if socket.send(frame).await.is_ok() {
                        let _ = timeout(CLOSE_WAIT, async { while let Some(Ok(_)) = socket.recv().await {} }).await;
                    }
                    return;
                }
                Some(frame) => {
                    if socket.send(frame).await.is_err() {
                        return;
                    }
                }
                None => return,
            },
            frame = socket.recv() => match frame {
                Some(Ok(frame)) => {
                    if incoming.send(frame).is_err() {
                        return;
                    }
                }
                Some(Err(_)) | None => return,
            },
        }
    }
}
