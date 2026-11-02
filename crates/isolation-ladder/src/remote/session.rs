//! Une connexion `isolation-remote` : poignée de main, `hello`, puis attente des parties. Le thread
//! de la session est le seul à lire et écrire sa WebSocket ; l'arène lui confie chaque partie.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message as Frame, WebSocket, close_code};
use axum::http::StatusCode;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::time::timeout;

use isolation_core::remote::{Message, PROTOCOL_VERSION, is_build};

use super::stream::RefereeStream;
use super::{GameSpec, Registry};
use crate::submit::{bearer, find_team, load_teams};
use crate::sync::lock;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_POLL: Duration = Duration::from_millis(50);
/// Attente maximale d'une ligne de l'arbitre avant d'être relayée.
const GAME_POLL: Duration = Duration::from_millis(5);
const PING_EVERY: Duration = Duration::from_secs(10);
/// Sans nouvelles du client depuis ce délai, la session tombe.
const SILENCE_LIMIT: Duration = Duration::from_secs(30);
/// Une session n'entre au tirage que si son client a répondu depuis ce délai : une connexion morte
/// en silence (portable en veille) en sort vite.
const FRESH: Duration = Duration::from_secs(15);
/// Dernières lignes de l'arbitre encore en route quand l'arène termine la partie.
const END_GRACE: Duration = Duration::from_millis(500);
pub const MAX_MESSAGE: usize = 64 << 10;
/// Close reasons travel in one control frame.
const MAX_REASON: usize = 123;
const CLOSE_WAIT: Duration = Duration::from_secs(1);
/// Une partie dure quelques minutes, à un ping toutes les 10 s.
const MAX_RTT_SAMPLES: usize = 64;
/// Texte du client repris dans les journaux.
const MAX_CLIENT_TEXT: usize = 200;

pub struct Start {
    pub game: GameSpec,
    pub over: Arc<AtomicBool>,
    /// Lâché à la fin du relais.
    pub done: Sender<()>,
}

pub struct Session {
    pub team: String,
    starts: Sender<Start>,
    state: Mutex<State>,
}

struct State {
    playing: bool,
    closing: Option<String>,
    heard: Instant,
}

impl Session {
    pub fn available(&self) -> bool {
        let state = lock(&self.state);
        !state.playing && state.closing.is_none() && state.heard.elapsed() < FRESH
    }

    pub fn playing(&self) -> bool {
        lock(&self.state).playing
    }

    /// Fermée par son thread à sa prochaine attente.
    pub fn close(&self, reason: &str) {
        lock(&self.state).closing.get_or_insert_with(|| reason.to_string());
    }

    /// Marquée en partie dès l'envoi : une nouvelle connexion ne peut plus la remplacer.
    pub fn start(&self, start: Start) {
        lock(&self.state).playing = true;
        let _ = self.starts.send(start);
    }

    fn heard(&self) {
        lock(&self.state).heard = Instant::now();
    }

    fn silent_for(&self) -> Duration {
        lock(&self.state).heard.elapsed()
    }

    fn closing(&self) -> Option<String> {
        lock(&self.state).closing.clone()
    }
}

/// Toute la vie d'une connexion authentifiée.
pub fn run(socket: Socket, registry: &Arc<Registry>, team: &str, peer: &str, time_ms: u64) {
    let build = match hello(&socket) {
        Ok(build) => build,
        Err(reason) => {
            eprintln!("distant {team} refusé : {reason}");
            socket.goodbye(&reason);
            return;
        }
    };
    let (starts, games) = mpsc::channel();
    let session = Arc::new(Session {
        team: team.to_string(),
        starts,
        state: Mutex::new(State {
            playing: false,
            closing: None,
            heard: Instant::now(),
        }),
    });
    if let Err(reason) = registry.open(&session, &build) {
        eprintln!("distant {team} refusé : {reason}");
        socket.goodbye(&reason);
        return;
    }
    eprintln!("distant : {team} connecté depuis {peer} (build {build})");
    let mut link = Link::new(socket, Arc::clone(&session));
    let reason = if link
        .send(&Message::Welcome {
            team: team.to_string(),
            time_ms,
        })
        .is_ok()
    {
        (registry.wake)();
        serve_games(&mut link, &session, &games)
    } else {
        None
    };
    registry.remove(&session);
    match reason {
        Some(reason) => {
            eprintln!("distant : {team} déconnecté ({reason})");
            link.socket.goodbye(&reason);
        }
        None => eprintln!("distant : {team} déconnecté"),
    }
}

/// Raison de fermer la session, `None` si le client est parti.
fn serve_games(link: &mut Link, session: &Session, games: &Receiver<Start>) -> Option<String> {
    loop {
        if let Ok(start) = games.try_recv() {
            let outcome = play(link, start);
            lock(&session.state).playing = false;
            match outcome {
                Outcome::Played => continue,
                Outcome::Failed(reason) => return Some(reason),
                Outcome::Gone => return None,
            }
        }
        if let Some(reason) = session.closing() {
            return Some(reason);
        }
        // Hors partie, un message du client n'a pas de sens : ignoré.
        if link.poll(IDLE_POLL).is_err() {
            return None;
        }
    }
}

/// Team of the bearer token, checked before the upgrade.
pub fn authenticate(teams: &Path, authorization: Option<&str>) -> Result<String, (StatusCode, String)> {
    let registered =
        load_teams(teams).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("registre des équipes : {e}")))?;
    authorization
        .and_then(bearer)
        .and_then(|token| find_team(&registered, token))
        .map(|found| found.id.as_str().to_string())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                "token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)".into(),
            )
        })
}

/// The session thread's end of the client's WebSocket, which an async pump serves.
pub struct Socket {
    incoming: Receiver<Frame>,
    outgoing: UnboundedSender<Frame>,
}

impl Socket {
    fn read(&self, wait: Duration) -> Result<Option<Frame>, Gone> {
        match self.incoming.recv_timeout(wait) {
            Ok(frame) => Ok(Some(frame)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(Gone),
        }
    }

    fn send(&self, frame: Frame) -> Result<(), Gone> {
        self.outgoing.send(frame).map_err(|_| Gone)
    }

    fn goodbye(&self, reason: &str) {
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

/// Build déclaré par le client.
fn hello(socket: &Socket) -> Result<String, String> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        let text = match socket.read(wait) {
            Ok(Some(Frame::Text(text))) => text,
            Ok(Some(Frame::Close(_)) | None) | Err(Gone) => return Err("hello jamais reçu".into()),
            Ok(Some(_)) => continue,
        };
        return match Message::from_json(text.as_str()) {
            Ok(Message::Hello { v, .. }) if v != PROTOCOL_VERSION => Err(format!(
                "protocole v{v} non géré, v{PROTOCOL_VERSION} attendu : mettre à jour isolation-remote"
            )),
            Ok(Message::Hello { build, .. }) if is_build(&build) => Ok(build),
            Ok(Message::Hello { .. }) => Err("build invalide : 16 caractères hexadécimaux attendus".into()),
            _ => Err("hello attendu en premier message".into()),
        };
    }
}

/// Texte du client sur une ligne, sans caractères de contrôle : il ne peut ni imiter une ligne du
/// journal du ladder ni piloter le terminal qui le lit.
fn printable(text: &str) -> String {
    text.chars()
        .take(MAX_CLIENT_TEXT)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// Le client est parti.
struct Gone;

/// La WebSocket d'une session : pings, silence du client, temps d'aller-retour.
struct Link {
    socket: Socket,
    session: Arc<Session>,
    since: Instant,
    pinged: Instant,
    /// Charge du ping sans réponse : un pong qui ne la reprend pas est ignoré.
    awaiting: Option<[u8; 8]>,
    /// Allers-retours mesurés pendant la partie en cours.
    rtt: Vec<Duration>,
}

impl Link {
    fn new(socket: Socket, session: Arc<Session>) -> Self {
        let now = Instant::now();
        Self {
            socket,
            session,
            since: now,
            pinged: now,
            awaiting: None,
            rtt: Vec::new(),
        }
    }

    /// Attend au plus `wait` un message du client.
    fn poll(&mut self, wait: Duration) -> Result<Option<Message>, Gone> {
        if self.session.silent_for() > SILENCE_LIMIT {
            return Err(Gone);
        }
        if self.pinged.elapsed() >= PING_EVERY {
            self.pinged = Instant::now();
            let sent = u64::try_from(self.since.elapsed().as_nanos())
                .unwrap_or(u64::MAX)
                .to_be_bytes();
            self.awaiting = Some(sent);
            self.socket.send(Frame::Ping(Bytes::copy_from_slice(&sent)))?;
        }
        match self.socket.read(wait)? {
            Some(frame) => {
                self.session.heard();
                self.frame(frame)
            }
            None => Ok(None),
        }
    }

    fn frame(&mut self, frame: Frame) -> Result<Option<Message>, Gone> {
        match frame {
            Frame::Text(text) => Ok(Message::from_json(text.as_str()).ok()),
            // Seule la réponse au ping en cours compte : un client ne peut pas remplir `rtt`.
            Frame::Pong(payload) => {
                if let Some(sent) = self.awaiting.filter(|sent| payload.as_ref() == sent) {
                    self.awaiting = None;
                    if self.rtt.len() < MAX_RTT_SAMPLES {
                        let sent = Duration::from_nanos(u64::from_be_bytes(sent));
                        self.rtt.push(self.since.elapsed().saturating_sub(sent));
                    }
                }
                Ok(None)
            }
            Frame::Close(_) => Err(Gone),
            _ => Ok(None),
        }
    }

    fn send(&mut self, message: &Message) -> Result<(), Gone> {
        self.socket.send(Frame::Text(message.to_json().into()))
    }
}

enum Outcome {
    Played,
    /// Le client n'a pas lancé son jar : la session se ferme avec cette raison.
    Failed(String),
    Gone,
}

/// Relie le client à l'arbitre le temps d'une partie. Le journal du joueur reçoit le diagnostic du
/// relais, comme la sortie d'erreur d'un jar.
fn play(link: &mut Link, start: Start) -> Outcome {
    let Start {
        game,
        over,
        done: _done,
    } = start;
    let mut log = File::create(&game.log).ok();
    let mut note = |text: &str| {
        if let Some(file) = &mut log {
            let _ = writeln!(file, "[relais] {text}");
        }
    };
    link.rtt.clear();
    let ask = Message::Game {
        id: game.id.clone(),
        side: game.side,
        time_ms: game.time_ms,
    };
    if link.send(&ask).is_err() {
        note("client déconnecté avant la partie");
        return Outcome::Gone;
    }
    let asked = Instant::now();
    loop {
        if over.load(Ordering::Relaxed) {
            note("jar jamais connecté au relais");
            return Outcome::Failed("jar jamais connecté au relais pendant la partie".into());
        }
        match link.poll(GAME_POLL) {
            Ok(Some(Message::Ready)) => break,
            Ok(Some(Message::Failed { reason })) => {
                let reason = printable(&reason);
                note(&format!("échec du client : {reason}"));
                return Outcome::Failed(format!("échec du client : {reason}"));
            }
            Ok(_) => {}
            Err(Gone) => {
                note("client déconnecté avant le lancement du jar");
                return Outcome::Gone;
            }
        }
    }
    note(&format!("jar prêt en {} ms", asked.elapsed().as_millis()));
    let (mut referee, lines) = match RefereeStream::connect(&game.connect).and_then(|s| Ok((s.try_clone()?, s))) {
        Ok((writer, reader)) => (writer, read_lines(reader)),
        Err(e) => {
            note(&format!("connexion à l'arbitre ({}) : {e}", game.connect));
            let _ = link.send(&Message::End { id: game.id });
            return Outcome::Played;
        }
    };
    let outcome = bridge(link, &mut referee, &lines, &over);
    referee.shutdown();
    match &outcome {
        Outcome::Played => {
            let _ = link.send(&Message::End { id: game.id });
        }
        _ => note("client déconnecté pendant la partie"),
    }
    let mut rtt = link.rtt.clone();
    rtt.sort_unstable();
    if let Some(median) = rtt.get(rtt.len() / 2) {
        note(&format!(
            "aller-retour médian {} ms ({} mesures)",
            median.as_millis(),
            rtt.len()
        ));
    }
    outcome
}

fn bridge(link: &mut Link, referee: &mut RefereeStream, lines: &Receiver<String>, over: &AtomicBool) -> Outcome {
    loop {
        loop {
            match lines.try_recv() {
                Ok(text) => {
                    if link.send(&Message::Line { text }).is_err() {
                        return Outcome::Gone;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Outcome::Played,
            }
        }
        if over.load(Ordering::Relaxed) {
            let deadline = Instant::now() + END_GRACE;
            while Instant::now() < deadline {
                match lines.recv_timeout(GAME_POLL) {
                    Ok(text) => {
                        if link.send(&Message::Line { text }).is_err() {
                            return Outcome::Gone;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            return Outcome::Played;
        }
        match link.poll(GAME_POLL) {
            Ok(Some(Message::Line { text })) => {
                // Arbitre parti : sa fin de connexion arrive par `lines`.
                let _ = referee.write_all(format!("{text}\n").as_bytes());
            }
            Ok(Some(Message::Hangup)) => referee.shutdown(),
            Ok(_) => {}
            Err(Gone) => return Outcome::Gone,
        }
    }
}

/// Lignes de l'arbitre, lues dans un thread ; le canal se ferme quand il raccroche.
fn read_lines(stream: RefereeStream) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(io::Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_text_stays_on_one_line_without_escapes() {
        let hostile = "\u{1b}[2J\u{1b}[31mFAUX\n== paire 99 : eq07 contre niveau-4\r";
        let clean = printable(hostile);
        assert!(!clean.chars().any(char::is_control), "{clean:?}");
        assert!(clean.contains("FAUX") && clean.contains("paire 99"));
        assert_eq!(printable(&"é".repeat(500)).chars().count(), MAX_CLIENT_TEXT);
    }
}
