//! Côté ladder : le registre des nœuds connectés. Chaque connexion est servie par un thread (comme
//! les équipes distantes) ; un nœud prêt et sain apporte ses slots au `Placement`, et
//! [`NodeExecutor`] y joue les parties que le ladder lui confie.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message as Frame, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::header::AUTHORIZATION;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;

use isolation_core::board::{ByColor, Color};
use isolation_core::digest::sha256_hex;
use isolation_core::record::{GameResult, RefereeEvent};

use super::protocol::{FromNode, Outcome, PROTOCOL, ToNode, WireGame, WirePlayer};
use crate::api::blocking;
use crate::arena::GameError;
use crate::bridge::{Gone, Socket};
use crate::exec::{Executor, GameOutput, GameRun};
use crate::placement::Placement;
use crate::roster::Team;
use crate::store::write_atomic;
use crate::submit::{bearer, find_team, load_teams};
use crate::sync::lock;

const HELLO_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(100);
const PING_EVERY: Duration = Duration::from_secs(2);
/// Sans nouvelles du nœud depuis ce délai, il est perdu.
const SILENCE: Duration = Duration::from_secs(6);
/// Après le délai d'une partie, le temps qu'on laisse au nœud pour répondre quand même.
const RESULT_GRACE: Duration = Duration::from_secs(10);
/// Échecs d'affilée avant la quarantaine, et sa durée.
const BREAKER_THRESHOLD: u32 = 3;
const QUARANTINE: Duration = Duration::from_mins(10);
/// Un message de nœud tient dans une trame : un résultat porte deux journaux de 1 Mio au plus.
pub const MAX_MESSAGE: usize = 4 << 20;

/// Disjoncteur d'un nœud : trop d'échecs d'affilée le sortent du placement un moment.
#[derive(Default)]
pub struct Breaker {
    failures: u32,
    until: Option<Instant>,
}

impl Breaker {
    /// Vrai si cet échec met le nœud en quarantaine.
    pub fn failure(&mut self, now: Instant) -> bool {
        self.failures += 1;
        if self.failures >= BREAKER_THRESHOLD {
            self.failures = 0;
            self.until = Some(now + QUARANTINE);
            return true;
        }
        false
    }

    pub const fn success(&mut self) {
        self.failures = 0;
    }

    pub fn quarantined(&self, now: Instant) -> bool {
        self.until.is_some_and(|until| now < until)
    }
}

/// Ce qu'une partie en cours reçoit de son nœud.
enum GameMsg {
    Event(RefereeEvent),
    Result(Outcome, ByColor<String>),
}

#[derive(Serialize, Clone, Default)]
struct NodeInfo {
    id: String,
    arch: String,
    version: String,
    slots: usize,
    ready: bool,
    running: usize,
    quarantined: bool,
    last_error: Option<String>,
}

/// Une connexion de nœud vivante.
pub struct NodeLink {
    outgoing: UnboundedSender<Frame>,
    games: Mutex<HashMap<String, Sender<GameMsg>>>,
    info: Mutex<NodeInfo>,
    breaker: Mutex<Breaker>,
    /// Ses slots sont dans le placement.
    placed: AtomicBool,
}

impl NodeLink {
    fn send(&self, message: &ToNode) -> Result<(), Gone> {
        let text = serde_json::to_string(message).map_err(|_| Gone)?;
        self.outgoing.send(Frame::text(text)).map_err(|_| Gone)
    }
}

pub struct NodeHub {
    placement: Arc<Placement>,
    tokens: PathBuf,
    state_dir: PathBuf,
    links: Mutex<HashMap<String, Arc<NodeLink>>>,
    /// Jars des parties distantes en cours, par sha256 : ce que les nœuds peuvent demander.
    jars: Mutex<HashMap<String, PathBuf>>,
    draining: AtomicBool,
}

impl NodeHub {
    pub fn new(placement: Arc<Placement>, tokens: PathBuf, state_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            placement,
            tokens,
            state_dir,
            links: Mutex::new(HashMap::new()),
            jars: Mutex::new(HashMap::new()),
            draining: AtomicBool::new(false),
        })
    }

    pub fn link(&self, node: &str) -> Option<Arc<NodeLink>> {
        lock(&self.links).get(node).cloned()
    }

    /// L'exécuteur qui joue sur `node`.
    pub fn executor(self: &Arc<Self>, node: &str) -> Arc<dyn Executor> {
        Arc::new(NodeExecutor {
            hub: Arc::clone(self),
            node: node.into(),
        })
    }

    /// Le ladder s'arrête : chaque nœud finit ses parties et n'en prend plus.
    pub fn drain(&self) {
        self.draining.store(true, Ordering::SeqCst);
        for link in lock(&self.links).values() {
            let _ = link.send(&ToNode::Drain);
        }
    }

    /// Ajoute ou retire les slots d'un nœud selon qu'il est prêt et hors quarantaine.
    fn place(&self, node: &str, link: &NodeLink) {
        let (ready, slots) = {
            let info = lock(&link.info);
            (info.ready, info.slots)
        };
        let quarantined = lock(&link.breaker).quarantined(Instant::now());
        lock(&link.info).quarantined = quarantined;
        let wanted = ready && !quarantined && !self.draining.load(Ordering::SeqCst);
        if wanted != link.placed.swap(wanted, Ordering::SeqCst) {
            if wanted {
                self.placement.add_node(node, slots);
            } else {
                self.placement.remove_node(node);
            }
            self.publish();
        }
    }

    /// Compte le résultat d'une partie distante pour le disjoncteur du nœud.
    fn record(&self, node: &str, outcome: &Result<GameResult, GameError>) {
        let Some(link) = self.link(node) else {
            return;
        };
        match outcome {
            Ok(_) => lock(&link.breaker).success(),
            Err(e) => {
                lock(&link.info).last_error = Some(e.to_string());
                if lock(&link.breaker).failure(Instant::now()) {
                    eprintln!("nœud {node} en quarantaine {} min : {e}", QUARANTINE.as_secs() / 60);
                }
                self.place(node, &link);
            }
        }
    }

    /// `state/nodes.json`, privé : pour l'opérateur et une alerte « nœud absent ».
    fn publish(&self) {
        let mut nodes: Vec<NodeInfo> = lock(&self.links).values().map(|l| lock(&l.info).clone()).collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let json = serde_json::json!({ "v": 1, "nodes": nodes });
        if let Err(e) = write_atomic(&self.state_dir.join("nodes.json"), json.to_string().as_bytes()) {
            eprintln!("   nodes.json non publié : {e}");
        }
    }
}

/// Le nœud du token, ou le refus à renvoyer. Même registre et même comparaison que les équipes.
pub fn authenticate(tokens: &Path, authorization: Option<&str>) -> Result<String, (StatusCode, String)> {
    let registered =
        load_teams(tokens).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("registre des nœuds : {e}")))?;
    authorization
        .and_then(bearer)
        .and_then(|token| find_team(&registered, token))
        .map(|found| found.id.as_str().to_string())
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "token de nœud absent ou inconnu".into()))
}

pub async fn upgrade(
    State(hub): State<Arc<NodeHub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let tokens = hub.tokens.clone();
    let node = match blocking(move || authenticate(&tokens, authorization.as_deref())).await {
        Ok(node) => node,
        Err((status, reason)) => {
            eprintln!("nœud {peer} refusé : {reason}");
            return (status, format!("{reason}\n")).into_response();
        }
    };
    upgrade
        .max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| async move {
            let (socket, pump) = crate::bridge::connect(socket);
            thread::spawn(move || serve(&socket, &hub, &node));
            pump.await;
        })
}

/// Sert une connexion du nœud `node` (déjà authentifié) jusqu'à sa fin.
pub fn serve(socket: &Socket, hub: &Arc<NodeHub>, node: &str) {
    let slots = match hello(socket, node) {
        Ok(slots) => slots,
        Err(reason) => {
            eprintln!("nœud {node} refusé : {reason}");
            socket.goodbye(&reason);
            return;
        }
    };
    let link = Arc::new(NodeLink {
        outgoing: socket.sender(),
        games: Mutex::new(HashMap::new()),
        info: Mutex::new(NodeInfo {
            id: node.into(),
            arch: slots.1,
            version: env!("CARGO_PKG_VERSION").into(),
            slots: slots.0,
            ..NodeInfo::default()
        }),
        breaker: Mutex::new(Breaker::default()),
        placed: AtomicBool::new(false),
    });
    if let Some(old) = lock(&hub.links).insert(node.into(), Arc::clone(&link)) {
        // Une reconnexion remplace l'ancienne : ses parties sont perdues avec elle.
        let _ = old.outgoing.send(Frame::Close(None));
        hub.placement.remove_node(node);
        lock(&old.games).clear();
    }
    eprintln!("nœud {node} connecté ({} slots)", slots.0);
    hub.publish();
    if hub.draining.load(Ordering::SeqCst) {
        let _ = link.send(&ToNode::Drain);
    }
    let why = run(socket, hub, node, &link);
    eprintln!("nœud {node} parti : {why}");
    let mut links = lock(&hub.links);
    if links.get(node).is_some_and(|l| Arc::ptr_eq(l, &link)) {
        links.remove(node);
        if link.placed.load(Ordering::SeqCst) {
            hub.placement.remove_node(node);
        }
    }
    drop(links);
    // Fermer les canaux des parties en cours : leur exécuteur rend `NodeLost`.
    lock(&link.games).clear();
    hub.publish();
}

/// Le `hello` du nœud : ses slots et son architecture, ou la raison du refus.
fn hello(socket: &Socket, node: &str) -> Result<(usize, String), String> {
    let deadline = Instant::now() + HELLO_TIMEOUT;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        if wait.is_zero() {
            return Err("pas de hello".into());
        }
        let Ok(Some(frame)) = socket.read(wait) else {
            continue;
        };
        let Frame::Text(text) = frame else {
            continue;
        };
        let Ok(FromNode::Hello {
            node: claimed,
            version,
            protocol,
            arch,
            slots,
        }) = serde_json::from_str(text.as_str())
        else {
            return Err("premier message autre que hello".into());
        };
        if claimed != node {
            return Err(format!("se présente comme {claimed:?} avec le token de {node:?}"));
        }
        if version != env!("CARGO_PKG_VERSION") || protocol != PROTOCOL {
            return Err(format!(
                "version {version} (protocole {protocol}) ; le ladder est en {} (protocole {PROTOCOL})",
                env!("CARGO_PKG_VERSION")
            ));
        }
        return Ok((slots, arch));
    }
}

fn run(socket: &Socket, hub: &Arc<NodeHub>, node: &str, link: &Arc<NodeLink>) -> String {
    let mut heard = Instant::now();
    let mut pinged = Instant::now();
    loop {
        if pinged.elapsed() >= PING_EVERY {
            pinged = Instant::now();
            if socket.send(Frame::Ping(Vec::new().into())).is_err() {
                return "connexion fermée".into();
            }
            // La quarantaine finit d'elle-même : le nœud revient au prochain tour.
            hub.place(node, link);
        }
        if heard.elapsed() >= SILENCE {
            return format!("aucune nouvelle depuis {} s", SILENCE.as_secs());
        }
        match socket.read(POLL) {
            Err(Gone) => return "connexion fermée".into(),
            Ok(None) => {}
            Ok(Some(frame)) => {
                heard = Instant::now();
                if let Err(why) = on_frame(hub, node, link, frame) {
                    return why;
                }
            }
        }
    }
}

/// Une trame du nœud ; `Err` : la raison de fin de la connexion.
fn on_frame(hub: &Arc<NodeHub>, node: &str, link: &Arc<NodeLink>, frame: Frame) -> Result<(), String> {
    match frame {
        Frame::Text(text) => match serde_json::from_str::<FromNode>(text.as_str()) {
            Ok(message) => on_message(hub, node, link, message),
            Err(e) => eprintln!("nœud {node} : message ignoré ({e})"),
        },
        Frame::Close(_) => return Err("fermée par le nœud".into()),
        _ => {}
    }
    Ok(())
}

fn on_message(hub: &Arc<NodeHub>, node: &str, link: &Arc<NodeLink>, message: FromNode) {
    match message {
        FromNode::Hello { .. } => {}
        FromNode::Ready { ready, why } => {
            {
                let mut info = lock(&link.info);
                info.ready = ready;
                if let Some(why) = why {
                    eprintln!("nœud {node} pas prêt : {why}");
                    info.last_error = Some(why);
                }
            }
            hub.place(node, link);
            hub.publish();
        }
        FromNode::Heartbeat { running, .. } => lock(&link.info).running = running,
        FromNode::Event { game, event } => {
            if let Some(tx) = lock(&link.games).get(&game) {
                let _ = tx.send(GameMsg::Event(event));
            }
        }
        FromNode::Result { game, outcome, logs } => {
            if let Some(tx) = lock(&link.games).remove(&game) {
                let _ = tx.send(GameMsg::Result(outcome, logs));
            }
        }
        FromNode::Need { sha256 } => send_jar(hub, node, link, sha256),
    }
}

/// Le jar demandé, s'il appartient à une partie distante en cours ; sinon rien.
fn send_jar(hub: &NodeHub, node: &str, link: &NodeLink, sha256: String) {
    let path = lock(&hub.jars).get(&sha256).cloned();
    match path.map(fs::read) {
        Some(Ok(bytes)) => {
            let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            let _ = link.send(&ToNode::Jar { sha256, size });
            let _ = link.outgoing.send(Frame::Binary(bytes.into()));
        }
        Some(Err(e)) => eprintln!("nœud {node} : jar {sha256} illisible ({e})"),
        None => eprintln!("nœud {node} : jar {sha256} inconnu, ignoré"),
    }
}

/// Joue sur un nœud : envoie `start`, relaie le direct et les journaux, rend le résultat revalidé.
struct NodeExecutor {
    hub: Arc<NodeHub>,
    node: String,
}

impl NodeExecutor {
    /// Le joueur tel que le nœud le connaît : un bot par son nom, un jar par son sha256.
    fn wire(&self, team: &Team) -> Result<WirePlayer, GameError> {
        if let Some((strategy, args)) = team.cmd.bot() {
            return Ok(WirePlayer::Bot {
                strategy: strategy.into(),
                args: args.to_vec(),
            });
        }
        let remote = |message: String| GameError::Remote {
            node: self.node.clone(),
            message,
        };
        let path = team
            .build_file()
            .ok_or_else(|| remote(format!("{} : ni bot ni jar, ne peut pas jouer sur un nœud", team.id)))?;
        let bytes = fs::read(&path).map_err(|e| remote(format!("{} : {e}", path.display())))?;
        let sha256 = sha256_hex(&bytes);
        lock(&self.hub.jars).insert(sha256.clone(), path);
        Ok(WirePlayer::Jar { sha256 })
    }

    fn play(&self, run: &GameRun<'_>, out: &dyn GameOutput) -> Result<GameResult, GameError> {
        let lost = || GameError::NodeLost(self.node.clone());
        let link = self.hub.link(&self.node).ok_or_else(lost)?;
        let start = WireGame {
            game: run.id.into(),
            slot: run.slot,
            time_ms: run.time_ms,
            opening: run.opening.iter().map(ToString::to_string).collect(),
            timeout_ms: u64::try_from(run.timeout.as_millis()).unwrap_or(u64::MAX),
            red: self.wire(run.red)?,
            black: self.wire(run.black)?,
        };
        let (tx, rx) = mpsc::channel();
        lock(&link.games).insert(run.id.into(), tx);
        link.send(&ToNode::Start(start)).map_err(|_| lost())?;
        drop(link);
        self.wait(run, out, &rx)
    }

    fn wait(&self, run: &GameRun<'_>, out: &dyn GameOutput, rx: &Receiver<GameMsg>) -> Result<GameResult, GameError> {
        let deadline = Instant::now() + run.timeout;
        loop {
            let wait = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(wait) {
                Ok(GameMsg::Event(event)) => out.event(event),
                Ok(GameMsg::Result(outcome, logs)) => {
                    for side in [Color::Red, Color::Black] {
                        let _ = fs::write(out.log_path(side), &logs[side]);
                    }
                    return outcome.into_result(&self.node);
                }
                Err(RecvTimeoutError::Disconnected) => return Err(GameError::NodeLost(self.node.clone())),
                Err(RecvTimeoutError::Timeout) => {
                    self.abort(run.id);
                    // Le nœud tue la partie ; son résultat tardif est sans objet.
                    let _ = rx.recv_timeout(RESULT_GRACE.min(run.timeout));
                    if let Some(link) = self.hub.link(&self.node) {
                        lock(&link.games).remove(run.id);
                    }
                    return Err(GameError::Timeout(run.timeout));
                }
            }
        }
    }
}

impl Executor for NodeExecutor {
    fn run(&self, run: &GameRun<'_>, out: Arc<dyn GameOutput>) -> Result<GameResult, GameError> {
        let outcome = self.play(run, out.as_ref());
        for team in [run.red, run.black] {
            if team.cmd.bot().is_none()
                && let Some(path) = team.build_file()
            {
                lock(&self.hub.jars).retain(|_, p| *p != path);
            }
        }
        self.hub.record(&self.node, &outcome);
        outcome
    }

    fn abort(&self, game: &str) {
        if let Some(link) = self.hub.link(&self.node) {
            let _ = link.send(&ToNode::Abort { game: game.into() });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::Sender;

    use isolation_core::digest::sha256_hex;
    use isolation_core::record::{InvalidCounts, RECORD_VERSION, Reason};
    use tokio::sync::mpsc::UnboundedReceiver;

    use super::*;
    use crate::config::Config;
    use crate::exec::tests::Seen;
    use crate::placement::PairClass;
    use crate::roster::Team;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("isolation-hub-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn hub(dir: &Path) -> Arc<NodeHub> {
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n".parse().unwrap();
        NodeHub::new(Placement::new(&cfg), dir.join("tokens.toml"), dir.to_path_buf())
    }

    /// L'autre bout d'une connexion de nœud.
    struct Peer {
        to_hub: Sender<Frame>,
        from_hub: UnboundedReceiver<Frame>,
    }

    impl Peer {
        fn say(&self, message: &FromNode) {
            self.to_hub
                .send(Frame::text(serde_json::to_string(message).unwrap()))
                .unwrap();
        }

        /// Le prochain message texte du ladder (pings ignorés).
        fn hear(&mut self) -> Frame {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match self.from_hub.try_recv() {
                    Ok(Frame::Ping(_)) => {}
                    Ok(frame) => return frame,
                    Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
                    Err(e) => panic!("rien du ladder : {e}"),
                }
            }
        }

        fn hear_message(&mut self) -> ToNode {
            match self.hear() {
                Frame::Text(text) => serde_json::from_str(text.as_str()).unwrap(),
                other => panic!("attendu un message, reçu {other:?}"),
            }
        }
    }

    fn hello(node: &str, version: &str) -> FromNode {
        FromNode::Hello {
            node: node.into(),
            version: version.into(),
            protocol: PROTOCOL,
            arch: "aarch64".into(),
            slots: 2,
        }
    }

    /// Une connexion du nœud `node` servie dans un thread, comme après l'authentification.
    fn connect(hub: &Arc<NodeHub>, node: &str) -> (Peer, thread::JoinHandle<()>) {
        let (socket, to_hub, from_hub) = Socket::pair();
        let (hub, node) = (Arc::clone(hub), node.to_string());
        let served = thread::spawn(move || serve(&socket, &hub, &node));

        (Peer { to_hub, from_hub }, served)
    }

    fn ready(peer: &Peer) {
        peer.say(&hello("node-a", env!("CARGO_PKG_VERSION")));
        peer.say(&FromNode::Ready { ready: true, why: None });
    }

    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !check() {
            assert!(Instant::now() < deadline, "jamais : {what}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn result() -> GameResult {
        GameResult {
            v: RECORD_VERSION,
            time_ms: 100,
            opening: vec![],
            moves: vec![],
            winner: Color::Black,
            reason: Reason::Timeout,
            invalid: InvalidCounts::default(),
            duration_ms: 5,
            move_ms: vec![],
        }
    }

    fn bot(id: &str) -> Team {
        Team {
            id: id.into(),
            cmd: crate::command::CommandTemplate::new(["run.sh", "{side}", "--bot", "aleatoire"]),
            build: None,
            anchor: false,
            multithread: false,
            portable: true,
            remote: None,
        }
    }

    /// Joue `run` sur le nœud dans un thread ; rend le résultat par un canal.
    fn play(
        hub: &Arc<NodeHub>,
        red: Team,
        black: Team,
        timeout: Duration,
        seen: Arc<Seen>,
    ) -> Receiver<Result<GameResult, GameError>> {
        let executor = hub.executor("node-a");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let run = GameRun {
                id: "g1",
                slot: 1,
                time_ms: 100,
                opening: &[],
                red: &red,
                black: &black,
                timeout,
            };
            tx.send(executor.run(&run, seen as Arc<dyn GameOutput>)).unwrap();
        });
        rx
    }

    #[test]
    fn a_wrong_token_or_version_is_refused() {
        let dir = temp("refused");
        let token = crate::submit::add_team(&dir.join("tokens.toml"), "node-a".parse().unwrap()).unwrap();
        assert_eq!(
            authenticate(&dir.join("tokens.toml"), Some(&format!("Bearer {token}"))).unwrap(),
            "node-a"
        );
        assert!(authenticate(&dir.join("tokens.toml"), Some("Bearer faux")).is_err());
        assert!(authenticate(&dir.join("tokens.toml"), None).is_err());

        let hub = hub(&dir);
        for bad in [hello("node-a", "0.0.1"), hello("node-b", env!("CARGO_PKG_VERSION"))] {
            let (mut peer, served) = connect(&hub, "node-a");
            peer.say(&bad);
            assert!(matches!(peer.hear(), Frame::Close(_)));
            served.join().unwrap();
        }
        assert!(
            !hub.placement.fits(PairClass::Portable)
                || hub.placement.try_lease(PairClass::Portable).unwrap().slot().local()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ready_node_adds_portable_slots_and_loses_them_on_disconnect() {
        let dir = temp("ready");
        let hub = hub(&dir);
        let (peer, served) = connect(&hub, "node-a");
        peer.say(&hello("node-a", env!("CARGO_PKG_VERSION")));
        eventually("nœud connu", || {
            fs::read_to_string(dir.join("nodes.json")).is_ok_and(|s| s.contains("\"node-a\""))
        });
        let first = hub.placement.try_lease(PairClass::Portable).unwrap();
        assert!(first.slot().local(), "pas prêt : pas de slot de nœud");
        drop(first);
        peer.say(&FromNode::Ready { ready: true, why: None });
        eventually("slots ajoutés", || {
            hub.placement
                .try_lease(PairClass::Portable)
                .is_some_and(|l| !l.slot().local())
        });
        // nodes.json suit le placement de près, pas dans le même instant : on l'attend.
        let published = || {
            let text = fs::read_to_string(dir.join("nodes.json")).unwrap_or_default();
            serde_json::from_str::<serde_json::Value>(&text).unwrap_or_default()
        };
        eventually("publié prêt", || published()["nodes"][0]["ready"] == true);
        assert_eq!(published()["nodes"][0]["slots"], 2);

        drop(peer);
        served.join().unwrap();
        let lease = hub.placement.try_lease(PairClass::Portable).unwrap();
        assert!(lease.slot().local(), "nœud parti : ses slots aussi");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_remote_game_relays_events_and_returns_the_result() {
        let dir = temp("relay");
        let hub = hub(&dir);
        let (mut peer, _served) = connect(&hub, "node-a");
        ready(&peer);
        eventually("prêt", || {
            hub.placement.fits(PairClass::Portable) && hub.link("node-a").is_some_and(|l| lock(&l.info).ready)
        });
        let seen = Arc::new(Seen {
            dir: dir.clone(),
            ..Seen::default()
        });
        let played = play(
            &hub,
            bot("aleatoire"),
            bot("niveau-1"),
            Duration::from_secs(5),
            Arc::clone(&seen),
        );
        let ToNode::Start(start) = peer.hear_message() else {
            panic!("attendu start");
        };
        assert_eq!((start.game.as_str(), start.slot), ("g1", 1));
        assert_eq!(
            start.red,
            WirePlayer::Bot {
                strategy: "aleatoire".into(),
                args: vec![]
            }
        );
        peer.say(&FromNode::Event {
            game: "g1".into(),
            event: RefereeEvent::Ready,
        });
        peer.say(&FromNode::Result {
            game: "g1".into(),
            outcome: Outcome::Ok { result: result() },
            logs: ByColor {
                red: "rouge".into(),
                black: "noir".into(),
            },
        });
        assert_eq!(played.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(), result());
        assert_eq!(*seen.events.lock().unwrap(), [RefereeEvent::Ready]);
        assert_eq!(fs::read_to_string(dir.join("red.log")).unwrap(), "rouge");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_node_lost_mid_game_yields_node_lost() {
        let dir = temp("lost");
        let hub = hub(&dir);
        let (mut peer, served) = connect(&hub, "node-a");
        ready(&peer);
        eventually("prêt", || hub.link("node-a").is_some_and(|l| lock(&l.info).ready));
        let played = play(
            &hub,
            bot("a"),
            bot("b"),
            Duration::from_secs(30),
            Arc::new(Seen::default()),
        );
        assert!(matches!(peer.hear_message(), ToNode::Start(_)));
        drop(peer);
        served.join().unwrap();
        let outcome = played.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(outcome, Err(GameError::NodeLost(ref node)) if node == "node-a"),
            "{outcome:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hung_remote_game_times_out_and_aborts() {
        let dir = temp("hung");
        let hub = hub(&dir);
        let (mut peer, _served) = connect(&hub, "node-a");
        ready(&peer);
        eventually("prêt", || hub.link("node-a").is_some_and(|l| lock(&l.info).ready));
        let played = play(
            &hub,
            bot("a"),
            bot("b"),
            Duration::from_millis(200),
            Arc::new(Seen::default()),
        );
        assert!(matches!(peer.hear_message(), ToNode::Start(_)));
        let outcome = played.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(outcome, Err(GameError::Timeout(_))), "{outcome:?}");
        assert_eq!(peer.hear_message(), ToNode::Abort { game: "g1".into() });
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_jars_are_sent_once_on_need() {
        let dir = temp("need");
        let hub = hub(&dir);
        let jar = dir.join("current.jar");
        fs::write(&jar, b"un jar").unwrap();
        let sha = sha256_hex(b"un jar");
        let (mut peer, _served) = connect(&hub, "node-a");
        ready(&peer);
        eventually("prêt", || hub.link("node-a").is_some_and(|l| lock(&l.info).ready));
        let team = Team {
            build: Some(jar),
            cmd: crate::command::CommandTemplate::new(["run.sh", "{jar}"]),
            ..bot("eq07")
        };
        let _played = play(&hub, team, bot("b"), Duration::from_secs(30), Arc::new(Seen::default()));
        let ToNode::Start(start) = peer.hear_message() else {
            panic!("attendu start");
        };
        assert_eq!(start.red, WirePlayer::Jar { sha256: sha.clone() });
        peer.say(&FromNode::Need { sha256: sha.clone() });
        assert_eq!(
            peer.hear_message(),
            ToNode::Jar {
                sha256: sha.clone(),
                size: 6
            }
        );
        assert!(matches!(peer.hear(), Frame::Binary(bytes) if bytes.as_ref() == b"un jar"));
        peer.say(&FromNode::Need {
            sha256: "cd".repeat(32),
        });
        peer.say(&FromNode::Heartbeat {
            running: 1,
            temp_c: None,
        });
        thread::sleep(Duration::from_millis(100));
        assert!(
            peer.from_hub.try_recv().map_or(true, |f| matches!(f, Frame::Ping(_))),
            "jar inconnu : rien d'envoyé"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn three_failures_quarantine_a_node_for_ten_minutes() {
        let mut breaker = Breaker::default();
        let t0 = Instant::now();
        assert!(!breaker.failure(t0));
        assert!(!breaker.failure(t0));
        breaker.success();
        assert!(!breaker.failure(t0), "un succès remet le compte à zéro");
        assert!(!breaker.failure(t0));
        assert!(breaker.failure(t0), "troisième échec d'affilée : quarantaine");
        assert!(breaker.quarantined(t0 + Duration::from_secs(599)));
        assert!(!breaker.quarantined(t0 + Duration::from_secs(600)));
    }
}
