//! Une connexion `isolation-remote` : poignée de main, `hello`, puis attente des parties. Le thread
//! de la session est le seul à lire et écrire sa WebSocket ; l'arène lui confie chaque partie.

use std::fs::File;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::StatusCode;
use tungstenite::http::header::AUTHORIZATION;
use tungstenite::protocol::frame::coding::CloseCode;
use tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tungstenite::{Bytes, Error as WsError, Message as Frame, WebSocket};

use isolation::remote::{Message, PATH, PROTOCOL_VERSION, is_build};

use super::stream::RefereeStream;
use super::{GameSpec, Registry, lock};
use crate::submit::{find_team, load_teams};

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
const MAX_MESSAGE: usize = 64 << 10;
/// Limite du protocole WebSocket pour la raison d'une fermeture.
const MAX_REASON: usize = 123;

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

/// Toute la vie d'une connexion.
pub fn run(stream: TcpStream, registry: &Arc<Registry>, teams: &Path, time_ms: u64) {
    let peer = stream.peer_addr().map_or_else(|_| "?".into(), |a| a.to_string());
    let (mut ws, team) = match accept(stream, teams) {
        Ok(accepted) => accepted,
        Err(e) => {
            eprintln!("distant {peer} refusé : {e}");
            return;
        }
    };
    let build = match hello(&mut ws) {
        Ok(build) => build,
        Err(reason) => {
            eprintln!("distant {team} refusé : {reason}");
            goodbye(&mut ws, &reason);
            return;
        }
    };
    let (starts, games) = mpsc::channel();
    let session = Arc::new(Session {
        team: team.clone(),
        starts,
        state: Mutex::new(State {
            playing: false,
            closing: None,
            heard: Instant::now(),
        }),
    });
    if let Err(reason) = registry.open(&session, &build) {
        eprintln!("distant {team} refusé : {reason}");
        goodbye(&mut ws, reason);
        return;
    }
    eprintln!("distant : {team} connecté depuis {peer} (build {build})");
    let mut link = Link::new(ws, Arc::clone(&session));
    let reason = if link
        .send(&Message::Welcome {
            team: team.clone(),
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
            goodbye(&mut link.ws, &reason);
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

fn accept(stream: TcpStream, teams: &Path) -> Result<(WebSocket<TcpStream>, String), String> {
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .and_then(|()| stream.set_nodelay(true))
        .map_err(|e| e.to_string())?;
    let mut team = None;
    #[expect(clippy::result_large_err, reason = "type d'erreur imposé par tungstenite")]
    let check = |request: &Request, response: Response| {
        if request.uri().path() != PATH {
            return Err(refusal(StatusCode::NOT_FOUND, &format!("seul {PATH} est servi ici")));
        }
        let token = request
            .headers()
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|header| header.strip_prefix("Bearer "))
            .map(str::trim);
        let registered = load_teams(teams).map_err(|e| {
            refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("registre des équipes : {e}"),
            )
        })?;
        match token.and_then(|token| find_team(&registered, token)) {
            Some(found) => {
                team = Some(found.id.as_str().to_string());
                Ok(response)
            }
            None => Err(refusal(
                StatusCode::UNAUTHORIZED,
                "token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)",
            )),
        }
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    let ws = tungstenite::accept_hdr_with_config(stream, check, Some(config)).map_err(|e| e.to_string())?;
    let team = team.ok_or("poignée de main sans équipe")?;
    Ok((ws, team))
}

fn refusal(status: StatusCode, body: &str) -> ErrorResponse {
    let mut response = ErrorResponse::new(Some(format!("{body}\n")));
    *response.status_mut() = status;
    response
}

/// Build déclaré par le client.
fn hello(ws: &mut WebSocket<TcpStream>) -> Result<String, String> {
    loop {
        let text = match ws.read() {
            Ok(Frame::Text(text)) => text,
            Ok(Frame::Close(_)) | Err(_) => return Err("hello jamais reçu".into()),
            Ok(_) => continue,
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

/// Ferme proprement, raison comprise, sans attendre plus d'une seconde un client muet.
fn goodbye(ws: &mut WebSocket<TcpStream>, reason: &str) {
    let mut end = reason.len().min(MAX_REASON);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let frame = CloseFrame {
        code: CloseCode::Policy,
        reason: reason[..end].into(),
    };
    if ws.close(Some(frame)).is_err() {
        return;
    }
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

/// Le client est parti.
struct Gone;

/// La WebSocket d'une session : pings, silence du client, temps d'aller-retour.
struct Link {
    ws: WebSocket<TcpStream>,
    session: Arc<Session>,
    since: Instant,
    pinged: Instant,
    wait: Option<Duration>,
    /// Allers-retours mesurés pendant la partie en cours.
    rtt: Vec<Duration>,
}

impl Link {
    fn new(ws: WebSocket<TcpStream>, session: Arc<Session>) -> Self {
        let now = Instant::now();
        Self {
            ws,
            session,
            since: now,
            pinged: now,
            wait: None,
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
            let sent = u64::try_from(self.since.elapsed().as_nanos()).unwrap_or(u64::MAX);
            let payload = Bytes::copy_from_slice(&sent.to_be_bytes());
            self.ws.send(Frame::Ping(payload)).map_err(|_| Gone)?;
        }
        if self.wait != Some(wait) {
            self.ws.get_ref().set_read_timeout(Some(wait)).map_err(|_| Gone)?;
            self.wait = Some(wait);
        }
        match self.ws.read() {
            Ok(frame) => {
                self.session.heard();
                self.frame(frame)
            }
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(None),
            Err(_) => Err(Gone),
        }
    }

    fn frame(&mut self, frame: Frame) -> Result<Option<Message>, Gone> {
        match frame {
            Frame::Text(text) => Ok(Message::from_json(text.as_str()).ok()),
            Frame::Pong(payload) => {
                if let Ok(bytes) = <[u8; 8]>::try_from(payload.as_ref()) {
                    let sent = Duration::from_nanos(u64::from_be_bytes(bytes));
                    self.rtt.push(self.since.elapsed().saturating_sub(sent));
                }
                Ok(None)
            }
            Frame::Close(_) => Err(Gone),
            _ => Ok(None),
        }
    }

    fn send(&mut self, message: &Message) -> Result<(), Gone> {
        self.ws.send(Frame::text(message.to_json())).map_err(|_| Gone)
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
