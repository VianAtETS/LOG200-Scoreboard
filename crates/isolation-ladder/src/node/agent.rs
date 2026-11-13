//! L'agent d'un nœud (`isolation-ladder node node.toml`) : il se connecte au ladder, prouve qu'il sait
//! jouer (sonde bot contre bot), annonce ses slots puis joue les parties qu'on lui confie avec
//! l'exécuteur local, le même qu'au serveur principal. S'il perd le ladder, il arrête tout,
//! nettoie, et revient avec un délai croissant.
//!
//! Une seule boucle par connexion lit et écrit la WebSocket (lecture bornée à `TICK`) ; chaque
//! partie tourne dans son thread et lui parle par un canal.

use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Read};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Context;
use rand::{Rng, RngExt};
use tungstenite::client::IntoClientRequest;
use tungstenite::handshake::HandshakeError;
use tungstenite::http::HeaderValue;
use tungstenite::{Error as WsError, Message as Frame, WebSocket};

use isolation_core::board::{ByColor, Color, Move};
use isolation_core::record::RefereeEvent;

use super::config::NodeConfig;
use super::jars::JarCache;
use super::protocol::{FromNode, Outcome, PROTOCOL, ToNode, WireGame, WirePlayer, is_sha256};
use crate::exec::{Executor, GameOutput, GameRun, LOG_CAP, Local};
use crate::roster::Team;
use crate::sync::lock;
use crate::thermal::Thermal;

/// Préfixe des parties de la sonde de démarrage.
const PROBE: &str = "sonde";
const TICK: Duration = Duration::from_millis(100);
const HEARTBEAT: Duration = Duration::from_secs(2);
/// Sans nouvelles du ladder depuis ce délai, il est perdu.
const SILENCE: Duration = Duration::from_secs(6);
/// Après une sonde ratée, la suivante.
const REPROBE: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BACKOFF_CAP_S: u64 = 30;
/// Une session plus longue remet le délai de reconnexion à zéro.
const STABLE: Duration = Duration::from_secs(60);
/// Bornes d'une partie demandée : au-delà, ce n'est pas une partie du ladder.
const MAX_TIME_MS: u64 = 60_000;
const MAX_TIMEOUT_MS: u64 = 2 * 60 * 60 * 1000;

/// Une partie acceptée, prête à jouer : les commandes viennent de la config du nœud.
pub struct Accepted {
    pub game: String,
    pub slot: usize,
    pub time_ms: u64,
    pub opening: Vec<Move>,
    pub timeout: Duration,
    pub red: Team,
    pub black: Team,
}

#[derive(Debug)]
pub enum Refusal {
    /// Demande invalide : refusée sans rien lancer.
    Bad(String),
    /// Le jar de ce sha256 manque : le demander, la partie attend.
    NeedJar(String),
}

/// Un id de partie finit dans des chemins et des noms de conteneurs : rien qui puisse en sortir.
fn safe_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Une option de bot : `--un-nom` ou un nombre, rien que le shell ou Docker puisse interpréter.
fn safe_arg(arg: &str) -> bool {
    let flag = arg
        .strip_prefix("--")
        .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'));
    flag || (!arg.is_empty() && arg.len() <= 9 && arg.bytes().all(|b| b.is_ascii_digit()))
}

fn player(wire: &WirePlayer, cfg: &NodeConfig, jar: &impl Fn(&str) -> Option<PathBuf>) -> Result<Team, Refusal> {
    let cmd = match wire {
        WirePlayer::Bot { strategy, args } => {
            if !cfg.bots.contains(strategy) {
                return Err(Refusal::Bad(format!("bot {strategy:?} hors de la liste du nœud")));
            }
            if let Some(bad) = args.iter().find(|a| !safe_arg(a)) {
                return Err(Refusal::Bad(format!("option de bot refusée : {bad:?}")));
            }
            cfg.bot_cmd
                .then(std::iter::once(strategy.clone()).chain(args.iter().cloned()))
        }
        WirePlayer::Jar { sha256 } => {
            if !is_sha256(sha256) {
                return Err(Refusal::Bad("sha256 invalide".into()));
            }
            let path = jar(sha256).ok_or_else(|| Refusal::NeedJar(sha256.clone()))?;
            cfg.team_cmd.with_jar(&path.to_string_lossy())
        }
    };
    let id = match wire {
        WirePlayer::Bot { strategy, .. } => strategy.clone(),
        WirePlayer::Jar { sha256 } => sha256[..12].to_string(),
    };
    Ok(Team {
        id,
        cmd,
        build: None,
        anchor: false,
        multithread: false,
        portable: true,
        remote: None,
    })
}

/// Vérifie une demande de partie et construit ses commandes. `jar` : le chemin d'un jar en cache.
pub fn validate(game: &WireGame, cfg: &NodeConfig, jar: impl Fn(&str) -> Option<PathBuf>) -> Result<Accepted, Refusal> {
    let bad = |why: &str| Err(Refusal::Bad(why.into()));
    if !safe_id(&game.game) {
        return bad("id de partie invalide");
    }
    if game.slot >= cfg.slots() {
        return bad("slot inexistant sur ce nœud");
    }
    if game.time_ms == 0 || game.time_ms > MAX_TIME_MS || game.timeout_ms > MAX_TIMEOUT_MS {
        return bad("temps hors bornes");
    }
    let Ok(opening) = game
        .opening
        .iter()
        .map(|m| m.parse::<Move>())
        .collect::<Result<Vec<_>, _>>()
    else {
        return bad("ouverture illisible");
    };
    Ok(Accepted {
        game: game.game.clone(),
        slot: game.slot,
        time_ms: game.time_ms,
        opening,
        timeout: Duration::from_millis(game.timeout_ms),
        red: player(&game.red, cfg, &jar)?,
        black: player(&game.black, cfg, &jar)?,
    })
}

/// Délai avant la reconnexion `attempt` : 1, 2, 4... s, plafonné à 30 s, à ±20 % près pour que
/// deux nœuds ne reviennent pas au même instant.
pub fn backoff(attempt: u32, rng: &mut impl Rng) -> Duration {
    let base = 2u64.saturating_pow(attempt).min(BACKOFF_CAP_S);
    Duration::from_secs(base).mul_f64(rng.random_range(0.8..=1.2))
}

#[derive(Debug)]
pub enum Ended {
    /// Le ladder a refusé la connexion (token, version).
    Refused(String),
    Lost(String),
}

impl std::fmt::Display for Ended {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(why) => write!(f, "refusé par le ladder : {why}"),
            Self::Lost(why) => write!(f, "ladder perdu : {why}"),
        }
    }
}

/// Lance l'agent jusqu'à l'arrêt du processus.
pub fn run(cfg: NodeConfig) -> anyhow::Result<()> {
    let agent = prepare(cfg)?;
    let mut rng = rand::rng();
    let mut attempt = 0;
    loop {
        let since = Instant::now();
        eprintln!("{}", agent.session());
        attempt = next_attempt(attempt, since.elapsed());
        thread::sleep(backoff(attempt, &mut rng));
    }
}

/// Le token, le dossier des journaux, un Docker interne sans restes d'un arrêt brutal.
fn prepare(cfg: NodeConfig) -> anyhow::Result<Agent> {
    let token = fs::read_to_string(&cfg.token_file)
        .with_context(|| cfg.token_file.display().to_string())?
        .trim()
        .to_string();
    fs::create_dir_all(&cfg.logs).with_context(|| cfg.logs.display().to_string())?;
    let local = Arc::new(Local::new(cfg.exec.clone()));
    local.sweep();
    let jars = cfg.jars.clone();
    Agent::new(cfg, token, local, jars).context("cache de jars")
}

/// Une session qui a tenu longtemps remet le délai de reconnexion à zéro.
fn next_attempt(attempt: u32, lasted: Duration) -> u32 {
    if lasted > STABLE { 0 } else { attempt.saturating_add(1) }
}

pub struct Agent {
    cfg: NodeConfig,
    token: String,
    exec: Arc<dyn Executor>,
    jars: JarCache,
    thermal: Mutex<Option<Thermal>>,
}

impl Agent {
    pub fn new(cfg: NodeConfig, token: String, exec: Arc<dyn Executor>, jars: PathBuf) -> std::io::Result<Self> {
        let jars = JarCache::new(jars, cfg.cache_mb << 20, cfg.max_jar_mb << 20)?;
        let thermal = Mutex::new(Thermal::new(cfg.thermal.as_ref()));
        Ok(Self {
            cfg,
            token,
            exec,
            jars,
            thermal,
        })
    }

    /// Une connexion au ladder, jusqu'à sa perte. Les parties en cours sont arrêtées avant de rendre.
    pub fn session(&self) -> Ended {
        let ws = match self.connect() {
            Ok(ws) => ws,
            Err(ended) => return ended,
        };
        let (notes_tx, notes) = mpsc::channel();
        let mut session = Session {
            agent: self,
            ws,
            notes_tx,
            notes,
            games: HashMap::new(),
            waiting: HashMap::new(),
            jar_coming: None,
            probe: Probe::Running,
            probed_at: Instant::now(),
            cool: true,
            ready: false,
            draining: false,
            heard: Instant::now(),
            beat: Instant::now(),
        };
        let ended = session.serve();
        for game in session.games.keys() {
            self.exec.abort(game);
        }
        for (_, (_, handle)) in session.games.drain() {
            let _ = handle.join();
        }
        ended
    }

    fn connect(&self) -> Result<WebSocket<TcpStream>, Ended> {
        let mut request = self
            .cfg
            .ladder
            .as_str()
            .into_client_request()
            .map_err(|e| Ended::Refused(e.to_string()))?;
        let bearer = HeaderValue::from_str(&format!("Bearer {}", self.token))
            .map_err(|_| Ended::Refused("token illisible".into()))?;
        request.headers_mut().insert("Authorization", bearer);
        let uri = request.uri();
        let host = uri.host().unwrap_or_default().to_string();
        let port = uri.port_u16().unwrap_or(80);
        let address = (host.as_str(), port)
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next())
            .ok_or_else(|| Ended::Lost(format!("{host}:{port} introuvable")))?;
        let stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|e| Ended::Lost(e.to_string()))?;
        let _ = stream.set_nodelay(true);
        let (ws, _) = tungstenite::client(request, stream).map_err(|e| match e {
            HandshakeError::Failure(WsError::Http(response)) if response.status().is_client_error() => {
                let body = response
                    .body()
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .unwrap_or_default();
                Ended::Refused(format!("{} {}", response.status(), body.trim()))
            }
            other => Ended::Lost(other.to_string()),
        })?;
        ws.get_ref()
            .set_read_timeout(Some(TICK))
            .map_err(|e| Ended::Lost(e.to_string()))?;
        Ok(ws)
    }
}

enum Probe {
    Running,
    Passed,
    Failed(String),
}

/// Ce que les threads de partie et de sonde disent à la boucle.
enum Note {
    Send(FromNode),
    Probed(Result<(), String>),
    Finished(String),
}

struct Session<'a> {
    agent: &'a Agent,
    ws: WebSocket<TcpStream>,
    notes_tx: Sender<Note>,
    notes: Receiver<Note>,
    /// Parties en cours : leur slot et leur thread.
    games: HashMap<String, (usize, JoinHandle<()>)>,
    /// Parties qui attendent leur jar, par sha256.
    waiting: HashMap<String, Vec<WireGame>>,
    /// Annonce du jar de la prochaine trame binaire.
    jar_coming: Option<String>,
    probe: Probe,
    probed_at: Instant,
    cool: bool,
    ready: bool,
    draining: bool,
    heard: Instant,
    beat: Instant,
}

impl Session<'_> {
    fn serve(&mut self) -> Ended {
        let cfg = &self.agent.cfg;
        let hello = FromNode::Hello {
            node: cfg.id.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL,
            arch: std::env::consts::ARCH.into(),
            slots: cfg.slots(),
        };
        if let Err(ended) = self.send(&hello) {
            return ended;
        }
        self.start_probe();
        loop {
            if let Err(ended) = self.step() {
                return ended;
            }
        }
    }

    fn step(&mut self) -> Result<(), Ended> {
        while let Ok(note) = self.notes.try_recv() {
            self.on_note(note)?;
        }
        if self.beat.elapsed() >= HEARTBEAT {
            self.heartbeat()?;
        }
        if self.heard.elapsed() >= SILENCE {
            return Err(Ended::Lost(format!("aucune nouvelle depuis {} s", SILENCE.as_secs())));
        }
        self.listen()
    }

    fn on_note(&mut self, note: Note) -> Result<(), Ended> {
        match note {
            Note::Send(message) => self.send(&message),
            Note::Probed(probed) => {
                self.probed_at = Instant::now();
                self.probe = probed.map_or_else(
                    |why| {
                        eprintln!("sonde ratée : {why}");
                        Probe::Failed(why)
                    },
                    |()| Probe::Passed,
                );
                self.announce()
            }
            Note::Finished(game) => {
                if let Some((_, handle)) = self.games.remove(&game) {
                    let _ = handle.join();
                }
                Ok(())
            }
        }
    }

    /// Température, nouvelle sonde après un échec, état prêt, puis le battement lui-même.
    fn heartbeat(&mut self) -> Result<(), Ended> {
        self.beat = Instant::now();
        self.cool = lock(&self.agent.thermal).as_mut().is_none_or(Thermal::cool);
        if matches!(self.probe, Probe::Failed(_)) && self.probed_at.elapsed() >= REPROBE {
            self.start_probe();
        }
        self.announce()?;
        let running = self.games.len();
        self.send(&FromNode::Heartbeat { running, temp_c: None })
    }

    fn listen(&mut self) -> Result<(), Ended> {
        match self.ws.read() {
            Ok(frame) => {
                self.heard = Instant::now();
                self.on_frame(frame)
            }
            Err(WsError::Io(e)) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Ok(()),
            Err(e) => Err(Ended::Lost(e.to_string())),
        }
    }

    fn on_frame(&mut self, frame: Frame) -> Result<(), Ended> {
        match frame {
            Frame::Text(text) => match serde_json::from_str::<ToNode>(text.as_str()) {
                Ok(message) => self.on_message(message),
                Err(e) => {
                    eprintln!("message du ladder ignoré : {e}");
                    Ok(())
                }
            },
            Frame::Binary(bytes) => self.on_jar(&bytes),
            Frame::Close(_) => Err(Ended::Lost("fermée par le ladder".into())),
            _ => Ok(()),
        }
    }

    fn on_message(&mut self, message: ToNode) -> Result<(), Ended> {
        match message {
            ToNode::Start(game) => self.start(game),
            ToNode::Jar { sha256, .. } => {
                self.jar_coming = Some(sha256);
                Ok(())
            }
            ToNode::Abort { game } => {
                if self.games.contains_key(&game) {
                    self.agent.exec.abort(&game);
                }
                Ok(())
            }
            ToNode::Drain => {
                self.draining = true;
                self.announce()
            }
        }
    }

    fn on_jar(&mut self, bytes: &[u8]) -> Result<(), Ended> {
        let Some(sha256) = self.jar_coming.take() else {
            return Ok(());
        };
        let games = self.waiting.remove(&sha256).unwrap_or_default();
        match self.agent.jars.put(&sha256, bytes) {
            Ok(_) => games.into_iter().try_for_each(|game| self.start(game)),
            Err(e) => games
                .into_iter()
                .try_for_each(|game| self.refuse(&game.game, &format!("jar refusé : {e}"))),
        }
    }

    fn start(&mut self, game: WireGame) -> Result<(), Ended> {
        if !self.ready {
            return self.refuse(&game.game, "nœud pas prêt");
        }
        if self.games.contains_key(&game.game) || self.games.values().any(|(slot, _)| *slot == game.slot) {
            return self.refuse(&game.game, "partie ou slot déjà en cours");
        }
        let jars = &self.agent.jars;
        match validate(&game, &self.agent.cfg, |sha256| jars.get(sha256)) {
            Ok(accepted) => {
                let id = accepted.game.clone();
                let slot = accepted.slot;
                let handle = self.spawn(accepted);
                self.games.insert(id, (slot, handle));
                Ok(())
            }
            Err(Refusal::NeedJar(sha256)) => {
                let first = !self.waiting.contains_key(&sha256);
                self.waiting.entry(sha256.clone()).or_default().push(game);
                if first {
                    self.send(&FromNode::Need { sha256 })?;
                }
                Ok(())
            }
            Err(Refusal::Bad(why)) => self.refuse(&game.game, &why),
        }
    }

    fn refuse(&mut self, game: &str, why: &str) -> Result<(), Ended> {
        self.send(&FromNode::Result {
            game: game.into(),
            outcome: Outcome::Error { message: why.into() },
            logs: ByColor::default(),
        })
    }

    fn spawn(&self, game: Accepted) -> JoinHandle<()> {
        let exec = Arc::clone(&self.agent.exec);
        let notes = self.notes_tx.clone();
        let dir = self.agent.cfg.logs.clone();
        thread::spawn(move || {
            let out = Arc::new(NodeOutput::new(&dir, &game.game, notes.clone()));
            let run = GameRun {
                id: &game.game,
                slot: game.slot,
                time_ms: game.time_ms,
                opening: &game.opening,
                red: &game.red,
                black: &game.black,
                timeout: game.timeout,
            };
            let outcome = match exec.run(&run, Arc::clone(&out) as Arc<dyn GameOutput>) {
                Ok(result) => Outcome::Ok { result },
                Err(e) => Outcome::from(&e),
            };
            let logs = out.take_logs();
            let _ = notes.send(Note::Send(FromNode::Result {
                game: game.game.clone(),
                outcome,
                logs,
            }));
            let _ = notes.send(Note::Finished(game.game));
        })
    }

    /// Joue une partie bot contre bot sur le slot 0 : Docker, gVisor et l'image répondent.
    fn start_probe(&mut self) {
        self.probe = Probe::Running;
        let cfg = &self.agent.cfg;
        let bot = WirePlayer::Bot {
            strategy: cfg.bots[0].clone(),
            args: vec![],
        };
        let id = format!("{PROBE}-{}-{}", cfg.id, std::process::id());
        let game = WireGame {
            game: id.clone(),
            slot: 0,
            time_ms: 1000,
            opening: vec![],
            timeout_ms: 300_000,
            red: bot.clone(),
            black: bot,
        };
        let accepted = validate(&game, cfg, |_| None);
        let exec = Arc::clone(&self.agent.exec);
        let notes = self.notes_tx.clone();
        let dir = cfg.logs.clone();
        thread::spawn(move || {
            let probed = accepted
                .map_err(|_| "la sonde elle-même est invalide".to_string())
                .and_then(|game| {
                    let out = Arc::new(NodeOutput::new(&dir, &id, mpsc::channel().0));
                    let run = GameRun {
                        id: &game.game,
                        slot: 0,
                        time_ms: game.time_ms,
                        opening: &[],
                        red: &game.red,
                        black: &game.black,
                        timeout: game.timeout,
                    };
                    let played = exec.run(&run, Arc::clone(&out) as Arc<dyn GameOutput>);
                    out.take_logs();
                    played.map(drop).map_err(|e| e.to_string())
                });
            let _ = notes.send(Note::Probed(probed));
        });
    }

    /// Prévient le ladder quand le nœud devient prêt ou ne l'est plus.
    fn announce(&mut self) -> Result<(), Ended> {
        let why = match &self.probe {
            _ if self.draining => Some("arrêt du ladder".to_string()),
            _ if !self.cool => Some("trop chaud".to_string()),
            Probe::Running => Some("sonde en cours".to_string()),
            Probe::Failed(why) => Some(format!("sonde ratée : {why}")),
            Probe::Passed => None,
        };
        let ready = why.is_none();
        if ready == self.ready {
            return Ok(());
        }
        self.ready = ready;
        self.send(&FromNode::Ready { ready, why })
    }

    fn send(&mut self, message: &FromNode) -> Result<(), Ended> {
        let text = serde_json::to_string(message).map_err(|e| Ended::Lost(e.to_string()))?;
        self.ws.send(Frame::text(text)).map_err(|e| Ended::Lost(e.to_string()))
    }
}

/// Les événements partent au ladder ; les journaux sont gardés ici jusqu'à la fin de la partie.
struct NodeOutput {
    game: String,
    notes: Sender<Note>,
    logs: ByColor<PathBuf>,
}

impl NodeOutput {
    fn new(dir: &Path, game: &str, notes: Sender<Note>) -> Self {
        let log = |side: &str| dir.join(format!("{game}-{side}.log"));
        Self {
            game: game.into(),
            notes,
            logs: ByColor {
                red: log("red"),
                black: log("black"),
            },
        }
    }

    /// Le contenu des journaux (tronqué comme au ladder), puis leurs fichiers effacés.
    fn take_logs(&self) -> ByColor<String> {
        let take = |path: &Path| {
            let mut bytes = Vec::new();
            if let Ok(file) = fs::File::open(path) {
                let _ = file.take(LOG_CAP as u64).read_to_end(&mut bytes);
            }
            let _ = fs::remove_file(path);
            String::from_utf8_lossy(&bytes).into_owned()
        };
        ByColor {
            red: take(&self.logs.red),
            black: take(&self.logs.black),
        }
    }
}

impl GameOutput for NodeOutput {
    fn event(&self, event: RefereeEvent) {
        let _ = self.notes.send(Note::Send(FromNode::Event {
            game: self.game.clone(),
            event,
        }));
    }

    fn log_path(&self, side: Color) -> PathBuf {
        self.logs[side].clone()
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::sync::mpsc::{self, Sender};

    use isolation_core::record::{GameResult, InvalidCounts, RECORD_VERSION, Reason};

    use rand::SeedableRng;
    use tungstenite::Message as Frame;

    use super::*;
    use crate::arena::GameError;
    use crate::node::config::tests::NODE;

    fn cfg() -> NodeConfig {
        NODE.parse().unwrap()
    }

    fn sha() -> String {
        "ab".repeat(32)
    }

    fn game(red: WirePlayer, black: WirePlayer) -> WireGame {
        WireGame {
            game: "20261113T120000.000Z-eq01-niveau-1".into(),
            slot: 1,
            time_ms: 1000,
            opening: vec!["G1-G4-F9".into()],
            timeout_ms: 264_000,
            red,
            black,
        }
    }

    fn bot(strategy: &str, args: &[&str]) -> WirePlayer {
        WirePlayer::Bot {
            strategy: strategy.into(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
        }
    }

    #[test]
    fn start_builds_commands_from_the_node_config() {
        let cfg = cfg();
        let cached = |_: &str| Some(PathBuf::from("/cache/x.jar"));
        let accepted = validate(
            &game(
                bot("niveau-1", &["--margin-ms", "100"]),
                WirePlayer::Jar { sha256: sha() },
            ),
            &cfg,
            cached,
        )
        .unwrap();
        assert_eq!(
            accepted.red.cmd.args(),
            ["bot", "{side}", "niveau-1", "--margin-ms", "100"]
        );
        assert!(accepted.black.cmd.args().iter().any(|a| a == "/cache/x.jar"));
        assert_eq!((accepted.slot, accepted.opening.len()), (1, 1));
    }

    #[test]
    fn start_rejects_unknown_bots_and_malformed_hashes() {
        let cfg = cfg();
        let cached = |_: &str| Some(PathBuf::from("/cache/x.jar"));
        let refused = |g: WireGame| matches!(validate(&g, &cfg, cached), Err(Refusal::Bad(_)));
        assert!(
            refused(game(bot("niveau-4", &[]), bot("aleatoire", &[]))),
            "hors liste blanche"
        );
        assert!(refused(game(bot("aleatoire", &["; rm -rf /"]), bot("aleatoire", &[]))));
        assert!(refused(game(bot("aleatoire", &["--x=$(id)"]), bot("aleatoire", &[]))));
        let jar = |sha256: &str| WirePlayer::Jar { sha256: sha256.into() };
        assert!(refused(game(jar("../../etc/passwd"), bot("aleatoire", &[]))));
        let mut far = game(bot("aleatoire", &[]), bot("aleatoire", &[]));
        far.slot = 2;
        assert!(refused(far), "slot hors bornes");
        let mut sneaky = game(bot("aleatoire", &[]), bot("aleatoire", &[]));
        sneaky.game = "../../srv/data".into();
        assert!(refused(sneaky), "l'id finit dans des chemins et des noms de conteneurs");
        let mut bad_opening = game(bot("aleatoire", &[]), bot("aleatoire", &[]));
        bad_opening.opening = vec!["$(reboot)".into()];
        assert!(refused(bad_opening));
        let missing = validate(&game(jar(&sha()), bot("aleatoire", &[])), &cfg, |_| None);
        assert!(matches!(missing, Err(Refusal::NeedJar(s)) if s == sha()));
    }

    #[test]
    fn backoff_grows_then_caps_with_jitter() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let first = backoff(0, &mut rng);
        assert!(
            first >= Duration::from_millis(800) && first <= Duration::from_millis(1200),
            "{first:?}"
        );
        for attempt in [5, 10, 40] {
            let wait = backoff(attempt, &mut rng);
            assert!(
                wait >= Duration::from_secs(24) && wait <= Duration::from_secs(36),
                "{wait:?}"
            );
        }
    }

    /// Joue jusqu'à `abort` ; la sonde (bot contre bot) finit tout de suite.
    struct Stub {
        started: Mutex<Option<Sender<String>>>,
        aborted: Mutex<Vec<String>>,
    }

    impl Executor for Stub {
        fn run(&self, run: &GameRun<'_>, _: Arc<dyn GameOutput>) -> Result<GameResult, GameError> {
            if run.id.starts_with(PROBE) {
                return Ok(GameResult {
                    v: RECORD_VERSION,
                    time_ms: run.time_ms,
                    opening: vec![],
                    moves: vec![],
                    winner: Color::Black,
                    reason: Reason::Timeout,
                    invalid: InvalidCounts::default(),
                    duration_ms: 1,
                    move_ms: vec![],
                });
            }
            if let Some(started) = self.started.lock().unwrap().as_ref() {
                started.send(run.id.to_string()).unwrap();
            }
            while !self.aborted.lock().unwrap().iter().any(|g| g == run.id) {
                thread::sleep(Duration::from_millis(10));
            }
            Err(GameError::Remote {
                node: "test".into(),
                message: "interrompue".into(),
            })
        }

        fn abort(&self, game: &str) {
            self.aborted.lock().unwrap().push(game.into());
        }
    }

    /// Le ladder des tests : il accepte l'agent (en vérifiant son token), lit et écrit des messages.
    struct FakeLadder(tungstenite::WebSocket<std::net::TcpStream>);

    impl FakeLadder {
        #[expect(clippy::result_large_err, reason = "signature du rappel imposée par tungstenite")]
        fn accept(listener: &TcpListener) -> Self {
            let (stream, _) = listener.accept().unwrap();
            let ws = tungstenite::accept_hdr(stream, |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(request.headers()["authorization"], "Bearer secret");
                Ok(response)
            })
            .unwrap();
            Self(ws)
        }

        fn read(&mut self) -> FromNode {
            loop {
                if let Frame::Text(text) = self.0.read().unwrap() {
                    return serde_json::from_str(text.as_str()).unwrap();
                }
            }
        }

        /// Le prochain message qui n'est pas un battement.
        fn next(&mut self) -> FromNode {
            loop {
                match self.read() {
                    FromNode::Heartbeat { .. } => {}
                    message => return message,
                }
            }
        }

        fn say(&mut self, message: &ToNode) {
            self.0
                .send(Frame::text(serde_json::to_string(message).unwrap()))
                .unwrap();
        }

        /// Le hello, puis la fin de la sonde.
        fn until_ready(&mut self) {
            assert!(matches!(self.read(), FromNode::Hello { slots: 2, .. }));
            while !matches!(self.read(), FromNode::Ready { ready: true, .. }) {}
        }
    }

    /// Un agent dont l'exécuteur est un bouchon, et le port où son ladder l'attend.
    fn agent(name: &str) -> (Agent, Arc<Stub>, TcpListener, Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut cfg = cfg();
        cfg.ladder = format!("ws://{}/api/node", listener.local_addr().unwrap());
        cfg.logs = std::env::temp_dir().join(format!("isolation-agent-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&cfg.logs);
        let (started_tx, started) = mpsc::channel();
        let stub = Arc::new(Stub {
            started: Mutex::new(Some(started_tx)),
            aborted: Mutex::new(Vec::new()),
        });
        let jars = cfg.logs.join("jars");
        let agent = Agent::new(cfg, "secret".into(), Arc::clone(&stub) as Arc<dyn Executor>, jars).unwrap();
        (agent, stub, listener, started)
    }

    #[test]
    fn agent_aborts_every_game_when_the_ladder_goes_away() {
        let (agent, stub, listener, started) = agent("away");
        let ladder = thread::spawn(move || {
            let mut ladder = FakeLadder::accept(&listener);
            ladder.until_ready();
            ladder.say(&ToNode::Start(game(bot("aleatoire", &[]), bot("niveau-1", &[]))));
            started.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let ended = agent.session();
        ladder.join().unwrap();
        assert!(matches!(ended, Ended::Lost(_)), "{ended:?}");
        assert_eq!(
            *stub.aborted.lock().unwrap(),
            ["20261113T120000.000Z-eq01-niveau-1"],
            "la partie en cours est arrêtée avant toute reconnexion"
        );
    }

    #[test]
    fn agent_fetches_missing_jars_then_obeys_abort_and_drain() {
        let (agent, stub, listener, started) = agent("jars");
        let jar = b"un jar".to_vec();
        let sha = isolation_core::digest::sha256_hex(&jar);
        let id = "20261113T120000.000Z-eq01-niveau-1";
        let ladder = thread::spawn(move || {
            let mut ladder = FakeLadder::accept(&listener);
            ladder.until_ready();
            ladder.say(&ToNode::Start(game(
                WirePlayer::Jar { sha256: sha.clone() },
                bot("niveau-1", &[]),
            )));
            assert_eq!(ladder.next(), FromNode::Need { sha256: sha.clone() });
            ladder.0.send(Frame::text("{\"type\":\"inconnu\"}")).unwrap();
            ladder.say(&ToNode::Jar { sha256: sha, size: 6 });
            ladder.0.send(Frame::Binary(jar.into())).unwrap();
            assert_eq!(
                started.recv_timeout(Duration::from_secs(5)).unwrap(),
                id,
                "le jar reçu, la partie part"
            );
            ladder.say(&ToNode::Abort { game: id.into() });
            let FromNode::Result {
                game: stopped, outcome, ..
            } = ladder.next()
            else {
                panic!("attendu le résultat de la partie arrêtée");
            };
            assert_eq!(stopped, id);

            assert!(matches!(outcome, Outcome::Error { .. }));
            ladder.say(&ToNode::Drain);
            assert!(matches!(ladder.next(), FromNode::Ready { ready: false, .. }));
            ladder.say(&ToNode::Start(game(bot("aleatoire", &[]), bot("niveau-1", &[]))));
            let FromNode::Result { outcome, .. } = ladder.next() else {
                panic!("attendu un refus");
            };
            assert!(matches!(outcome, Outcome::Error { message } if message.contains("pas prêt")));
        });
        let ended = agent.session();
        ladder.join().unwrap();
        assert!(matches!(ended, Ended::Lost(_)), "{ended:?}");
        assert_eq!(*stub.aborted.lock().unwrap(), [id]);
    }

    #[test]
    fn agent_beats_every_two_seconds_with_its_running_games() {
        let (agent, _stub, listener, _started) = agent("beat");
        let ladder = thread::spawn(move || {
            let mut ladder = FakeLadder::accept(&listener);
            ladder.until_ready();
            let since = Instant::now();
            loop {
                if let FromNode::Heartbeat { running, .. } = ladder.read() {
                    assert_eq!(running, 0);
                    break;
                }
            }
            assert!(since.elapsed() <= HEARTBEAT + Duration::from_secs(1));
        });
        let ended = agent.session();
        ladder.join().unwrap();
        assert!(matches!(ended, Ended::Lost(_)));
    }

    #[test]
    fn a_session_end_says_who_ended_it() {
        assert_eq!(Ended::Refused("401".into()).to_string(), "refusé par le ladder : 401");
        assert_eq!(Ended::Lost("délai".into()).to_string(), "ladder perdu : délai");
    }

    #[test]
    fn next_attempt_resets_after_a_stable_session() {
        assert_eq!(next_attempt(4, Duration::from_secs(5)), 5);
        assert_eq!(next_attempt(4, STABLE + Duration::from_secs(1)), 0);
        assert_eq!(next_attempt(u32::MAX, Duration::ZERO), u32::MAX);
    }

    #[test]
    fn prepare_reads_the_token_and_creates_the_logs() {
        let dir = std::env::temp_dir().join(format!("isolation-prepare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut node = cfg();
        node.token_file = dir.join("token");
        node.logs = dir.join("logs");
        node.jars = dir.join("jars");
        assert!(prepare(node).is_err(), "sans token, le nœud ne part pas");
        fs::write(
            dir.join("token"),
            "secret
",
        )
        .unwrap();
        let mut node = cfg();
        node.token_file = dir.join("token");
        node.logs = dir.join("logs");
        node.jars = dir.join("jars");
        let agent = prepare(node).unwrap();
        assert_eq!(agent.token, "secret");
        assert!(dir.join("logs").is_dir() && dir.join("jars").is_dir());
        let _ = fs::remove_dir_all(&dir);
    }
}
