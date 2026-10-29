//! Relais d'une équipe qui joue au ladder depuis sa machine (docs/formats.md, « Jouer à distance ») :
//! le jar tourne ici et se connecte à ce relais comme au serveur officiel. Seules les lignes du
//! protocole transitent, par une WebSocket vers le ladder ; le jar n'est jamais envoyé.

use std::fs;
use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Bytes, Error as WsError, Message as Frame, WebSocket};

use isolation::board::Color;
use isolation::digest::short_hash;
use isolation::remote::{Message, PATH, PROTOCOL_VERSION};

/// Attente maximale d'une ligne du ladder ou du jar avant d'être relayée.
const GAME_POLL: Duration = Duration::from_millis(5);
const IDLE_POLL: Duration = Duration::from_millis(100);
const PING_EVERY: Duration = Duration::from_secs(10);
const WELCOME_WAIT: Duration = Duration::from_secs(10);
/// Au-delà, l'arbitre déclarerait l'équipe absente.
const JAR_CONNECT_WAIT: Duration = Duration::from_secs(10);
/// Le jar finit de lui-même après le message de fin ; sinon il est tué.
const JAR_EXIT_WAIT: Duration = Duration::from_secs(2);
const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Allers-retours retenus pour estimer le temps que la latence prend au jar.
const RTT_SAMPLES: usize = 5;

#[derive(Parser)]
#[command(
    name = "isolation-remote",
    version,
    about = "Joue au ladder Isolation depuis sa machine, sans téléverser son jar",
    after_help = "Exemple : LADDER_TOKEN=<token> isolation-remote --url https://ladder.example.org -- java -jar Client-eq07.jar"
)]
struct Cli {
    /// Adresse de la page du ladder, par exemple `https://ladder.example.org`
    #[arg(long)]
    url: String,
    /// Token de l'équipe. Par la variable d'environnement, il ne reste pas dans l'historique du shell.
    #[arg(long, env = "LADDER_TOKEN", hide_env_values = true)]
    token: String,
    /// Port local où le jar se connecte, comme au serveur officiel.
    #[arg(long, default_value_t = 8888)]
    port: u16,
    /// Commande qui lance le jar pour une partie, par exemple : java -jar Client-eq07.jar
    #[arg(last = true, required = true)]
    command: Vec<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let url = match endpoint(&cli.url) {
        Ok(url) => url,
        Err(e) => {
            say(&e);
            return ExitCode::from(2);
        }
    };
    let build = build_of(&cli.command);
    say(&format!("build {build} : {}", cli.command.join(" ")));
    let mut backoff = MIN_BACKOFF;
    loop {
        match session(&cli, &url, &build) {
            Ended::Lost { welcomed } => {
                if welcomed {
                    backoff = MIN_BACKOFF;
                }
                say(&format!("nouvelle tentative dans {} s", backoff.as_secs()));
                thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
            Ended::Closed(reason) => {
                say(&format!("session fermée par le ladder : {reason}"));
                return ExitCode::FAILURE;
            }
            Ended::Refused(reason) => {
                say(&format!("connexion refusée : {reason}"));
                return ExitCode::from(2);
            }
        }
    }
}

fn say(text: &str) {
    eprintln!("[relais] {text}");
}

/// `https://hote` devient `wss://hote/api/remote` ; `ws://` et `wss://` sont gardés.
fn endpoint(url: &str) -> Result<String, String> {
    let url = url.trim_end_matches('/');
    let base = if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else if url.starts_with("wss://") || url.starts_with("ws://") {
        url.to_string()
    } else {
        return Err(format!("adresse inattendue : {url} (https://… attendu)"));
    };
    Ok(if base.ends_with(PATH) {
        base
    } else {
        format!("{base}{PATH}")
    })
}

/// Comme le ladder : sha256 du jar de la commande, à défaut de la commande elle-même.
fn build_of(command: &[String]) -> String {
    let jar = command.iter().find(|arg| {
        Path::new(arg)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jar"))
    });
    match jar.and_then(|jar| fs::read(jar).ok()) {
        Some(bytes) => short_hash(&bytes),
        None => short_hash(command.join(" ").as_bytes()),
    }
}

enum Ended {
    /// Réseau coupé ou ladder injoignable : on réessaie.
    Lost { welcomed: bool },
    /// Le ladder a fermé la session, avec une raison à montrer à l'équipe.
    Closed(String),
    /// Token ou adresse refusés : réessayer ne changerait rien.
    Refused(String),
}

fn session(cli: &Cli, url: &str, build: &str) -> Ended {
    let mut request = match url.into_client_request() {
        Ok(request) => request,
        Err(e) => return Ended::Refused(e.to_string()),
    };
    match HeaderValue::from_str(&format!("Bearer {}", cli.token.trim())) {
        Ok(bearer) => request.headers_mut().insert("Authorization", bearer),
        Err(_) => return Ended::Refused("token illisible".into()),
    };
    let ws = match tungstenite::connect(request) {
        Ok((ws, _)) => ws,
        Err(WsError::Http(response)) if response.status().is_client_error() => {
            // Le ladder répond une ligne ; un autre serveur, peut-être toute une page.
            let body = response
                .body()
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let first = body.lines().map(str::trim).find(|line| !line.is_empty());
            let first: String = first.unwrap_or_default().chars().take(200).collect();
            return Ended::Refused(format!("{} {first}", response.status()));
        }
        Err(e) => {
            say(&format!("ladder injoignable : {e}"));
            return Ended::Lost { welcomed: false };
        }
    };
    let mut link = match Link::new(ws) {
        Ok(link) => link,
        Err(end) => return end.into_ended(false),
    };
    let hello = Message::Hello {
        v: PROTOCOL_VERSION,
        build: build.to_string(),
        client: format!("isolation-remote {}", env!("CARGO_PKG_VERSION")),
    };
    if let Err(end) = link.send(&hello) {
        return end.into_ended(false);
    }
    let deadline = Instant::now() + WELCOME_WAIT;
    loop {
        match link.poll(IDLE_POLL) {
            Ok(Some(Message::Welcome { team, time_ms })) => {
                say(&format!(
                    "connecté en tant que {team}, {time_ms} ms par coup : en attente d'une partie"
                ));
                break;
            }
            Ok(_) if Instant::now() < deadline => {}
            Ok(_) => return Ended::Lost { welcomed: false },
            Err(end) => return end.into_ended(false),
        }
    }
    loop {
        let game = match link.poll(IDLE_POLL) {
            Ok(Some(Message::Game { id, side, time_ms })) => Game { id, side, time_ms },
            Ok(_) => continue,
            Err(end) => return end.into_ended(true),
        };
        if let Err(end) = play(&mut link, cli, &game) {
            return end.into_ended(true);
        }
    }
}

/// Fin de la WebSocket.
enum End {
    Lost,
    Closed(String),
}

impl End {
    fn into_ended(self, welcomed: bool) -> Ended {
        match self {
            Self::Lost => Ended::Lost { welcomed },
            Self::Closed(reason) => Ended::Closed(reason),
        }
    }
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

struct Link {
    ws: Ws,
    since: Instant,
    pinged: Option<Instant>,
    wait: Option<Duration>,
    rtt: Vec<Duration>,
}

impl Link {
    fn new(ws: Ws) -> Result<Self, End> {
        tcp(&ws).set_nodelay(true).map_err(|_| End::Lost)?;
        Ok(Self {
            ws,
            since: Instant::now(),
            pinged: None,
            wait: None,
            rtt: Vec::new(),
        })
    }

    /// Le plus long des derniers allers-retours : la marge que la latence prend au jar.
    fn rtt(&self) -> Option<Duration> {
        self.rtt.iter().rev().take(RTT_SAMPLES).max().copied()
    }

    fn poll(&mut self, wait: Duration) -> Result<Option<Message>, End> {
        if self.pinged.is_none_or(|at| at.elapsed() >= PING_EVERY) {
            self.pinged = Some(Instant::now());
            let sent = u64::try_from(self.since.elapsed().as_nanos()).unwrap_or(u64::MAX);
            self.ws
                .send(Frame::Ping(Bytes::copy_from_slice(&sent.to_be_bytes())))
                .map_err(|e| lost(&e))?;
        }
        if self.wait != Some(wait) {
            tcp(&self.ws).set_read_timeout(Some(wait)).map_err(|_| End::Lost)?;
            self.wait = Some(wait);
        }
        match self.ws.read() {
            Ok(Frame::Text(text)) => Ok(Message::from_json(text.as_str()).ok()),
            Ok(Frame::Pong(payload)) => {
                if let Ok(bytes) = <[u8; 8]>::try_from(payload.as_ref()) {
                    let sent = Duration::from_nanos(u64::from_be_bytes(bytes));
                    self.rtt.push(self.since.elapsed().saturating_sub(sent));
                }
                Ok(None)
            }
            Ok(Frame::Close(frame)) => Err(End::Closed(frame.map(|f| f.reason.to_string()).unwrap_or_default())),
            Ok(_) => Ok(None),
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(lost(&e)),
        }
    }

    fn send(&mut self, message: &Message) -> Result<(), End> {
        self.ws.send(Frame::text(message.to_json())).map_err(|e| lost(&e))
    }
}

fn lost(error: &WsError) -> End {
    say(&format!("connexion au ladder perdue : {error}"));
    End::Lost
}

fn tcp(ws: &Ws) -> &TcpStream {
    match ws.get_ref() {
        MaybeTlsStream::Rustls(tls) => &tls.sock,
        MaybeTlsStream::Plain(stream) => stream,
        _ => unreachable!("seul rustls est compilé"),
    }
}

struct Game {
    id: String,
    side: Color,
    time_ms: u64,
}

/// Une partie : lance le jar, attend sa connexion, puis relaie ses lignes jusqu'à la fin.
fn play(link: &mut Link, cli: &Cli, game: &Game) -> Result<(), End> {
    let side = if game.side == Color::Red { "rouge" } else { "noir" };
    say(&format!("partie {} : on joue {side}", game.id));
    let (mut jar, mut child) = match launch(link, cli) {
        Ok(launched) => launched,
        Err(Launch::Failed(reason)) => {
            say(&reason);
            link.send(&Message::Failed { reason })?;
            // Le ladder ferme la session avec sa raison.
            loop {
                link.poll(IDLE_POLL)?;
            }
        }
        Err(Launch::Link(end)) => return Err(end),
    };
    link.send(&Message::Ready)?;
    let lines = read_lines(&jar);
    let mut hung_up = false;
    let mut last = String::new();
    let outcome = loop {
        loop {
            match lines.try_recv() {
                Ok(text) => link.send(&Message::Line { text })?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if !hung_up {
                        hung_up = true;
                        link.send(&Message::Hangup)?;
                    }
                    break;
                }
            }
        }
        match link.poll(GAME_POLL) {
            Ok(Some(Message::Line { text })) => {
                let text = announce(&text, link.rtt(), game.time_ms);
                let _ = jar.write_all(format!("{text}\n").as_bytes());
                if text.starts_with("5 ") {
                    last = text;
                }
            }
            Ok(Some(Message::End { .. })) => break Ok(()),
            Ok(_) => {}
            Err(end) => break Err(end),
        }
    };
    let _ = jar.shutdown(Shutdown::Both);
    stop(&mut child);
    say(&format!("partie {} terminée : {}", game.id, verdict(&last)));
    outcome
}

enum Launch {
    Failed(String),
    Link(End),
}

fn launch(link: &mut Link, cli: &Cli) -> Result<(TcpStream, Child), Launch> {
    let listener = TcpListener::bind(("127.0.0.1", cli.port))
        .and_then(|l| l.set_nonblocking(true).map(|()| l))
        .map_err(|e| Launch::Failed(format!("port {} indisponible : {e}", cli.port)))?;
    let mut child = Command::new(&cli.command[0])
        .args(&cli.command[1..])
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| Launch::Failed(format!("lancement de {} : {e}", cli.command[0])))?;
    let deadline = Instant::now() + JAR_CONNECT_WAIT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let ready = stream.set_nonblocking(false).and_then(|()| stream.set_nodelay(true));
                return match ready {
                    Ok(()) => Ok((stream, child)),
                    Err(e) => {
                        stop(&mut child);
                        Err(Launch::Failed(format!("connexion du jar : {e}")))
                    }
                };
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => {
                stop(&mut child);
                return Err(Launch::Failed(format!("connexion du jar : {e}")));
            }
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(Launch::Failed(format!(
                "le jar s'est terminé ({status}) sans se connecter au port {}",
                cli.port
            )));
        }
        if Instant::now() > deadline {
            stop(&mut child);
            return Err(Launch::Failed(format!(
                "le jar ne s'est pas connecté au port {} en {} s",
                cli.port,
                JAR_CONNECT_WAIT.as_secs()
            )));
        }
        // Garde la WebSocket vivante pendant le démarrage de la JVM.
        link.poll(Duration::from_millis(20)).map_err(Launch::Link)?;
    }
}

/// `0 2 12 12 T C` : le jar se voit annoncer `T` moins la latence, qui compte dans son temps.
fn announce(line: &str, rtt: Option<Duration>, time_ms: u64) -> String {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let (Some(rtt), ["0", version, rows, cols, announced, color]) = (rtt, fields.as_slice()) else {
        return line.to_string();
    };
    let Ok(announced) = announced.parse::<u64>() else {
        return line.to_string();
    };
    let margin = u64::try_from(rtt.as_millis()).unwrap_or(u64::MAX);
    let left = announced.saturating_sub(margin).max(announced / 2);
    if left != announced {
        say(&format!(
            "temps annoncé au jar : {left} ms ({time_ms} ms moins {margin} ms d'aller-retour)"
        ));
    }
    format!("0 {version} {rows} {cols} {left} {color}")
}

fn verdict(last: &str) -> &'static str {
    match last.split_whitespace().nth(1) {
        Some("WIN") => "victoire",
        Some("LOSS") => "défaite",
        Some("TIMEOUT") => "temps dépassé",
        _ => "fin sans résultat reçu",
    }
}

/// Lignes du jar, lues dans un thread ; le canal se ferme quand il raccroche.
fn read_lines(jar: &TcpStream) -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let reader = jar.try_clone();
    thread::spawn(move || {
        let Ok(reader) = reader else {
            return;
        };
        for line in BufReader::new(reader).lines().map_while(io::Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Laisse le jar finir de lui-même, puis le tue.
fn stop(child: &mut Child) {
    let deadline = Instant::now() + JAR_EXIT_WAIT;
    while Instant::now() < deadline {
        if !matches!(child.try_wait(), Ok(None)) {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_address_becomes_the_websocket() {
        assert_eq!(
            endpoint("https://ladder.example.org/").unwrap(),
            "wss://ladder.example.org/api/remote"
        );
        assert_eq!(
            endpoint("http://localhost:8080").unwrap(),
            "ws://localhost:8080/api/remote"
        );
        assert_eq!(endpoint("ws://h:8091/api/remote").unwrap(), "ws://h:8091/api/remote");
        assert!(endpoint("ladder.example.org").is_err());
    }

    #[test]
    fn announced_time_leaves_room_for_the_round_trip() {
        let rtt = Some(Duration::from_millis(40));
        assert_eq!(announce("0 2 12 12 500 1", rtt, 500), "0 2 12 12 460 1");
        assert_eq!(
            announce("0 2 12 12 500 1", Some(Duration::from_secs(1)), 500),
            "0 2 12 12 250 1"
        );
        assert_eq!(announce("0 2 12 12 500 1", None, 500), "0 2 12 12 500 1");
        assert_eq!(announce("3 G1 - E3 - C12", rtt, 500), "3 G1 - E3 - C12");
    }

    #[test]
    fn verdict_reads_the_last_message() {
        assert_eq!(verdict("5 WIN G1-E3-C12"), "victoire");
        assert_eq!(verdict("5 TIMEOUT"), "temps dépassé");
        assert_eq!(verdict(""), "fin sans résultat reçu");
    }
}
