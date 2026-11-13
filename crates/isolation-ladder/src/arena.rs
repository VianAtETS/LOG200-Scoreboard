//! Une partie du ladder : son slot, son id, ses entrées, son direct et son enregistrement. Le
//! déroulé lui-même est celui d'un exécuteur (`exec.rs`).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use thiserror::Error;

use isolation_core::board::{ByColor, Color, Move};
use isolation_core::digest::short_hash;
use isolation_core::record::{
    GameRecord, GameResult, Player, Reason, RefereeEvent, TournamentGame, ValidationError, now_rfc3339,
};

use crate::config::Config;
use crate::exec::{Executor, GameOutput, GameRun, Local};
use crate::hub::{self, Hub};
use crate::node::hub::NodeHub;
use crate::placement::{Lease, Placement, SlotId};

use crate::remote::Seat;
use crate::roster::Team;
use crate::store::compact_timestamp;

#[derive(Debug, Error)]
pub enum GameError {
    #[error("lancement de {who} : {error}")]
    Spawn { who: String, error: io::Error },
    #[error("arbitre : `{0}` jamais reçu")]
    MarkerNeverSeen(String),
    #[error("partie interrompue après {0:?}")]
    Timeout(Duration),
    #[error("attente de l'arbitre : {0}")]
    Wait(io::Error),
    #[error("arbitre terminé avec {0}")]
    RefereeFailed(ExitStatus),
    #[error("résultat illisible ({error}) : {json:?}")]
    BadResult { json: String, error: serde_json::Error },
    #[error("résultat incohérent : {0}")]
    Inconsistent(#[from] ValidationError),
    /// Partie de tournoi avant son dernier essai (`Fixture::forfeit_counts`).
    #[error("{0} jamais connecté à l'arbitre, partie rejouée")]
    NoShow(String),
    /// Le bac à sable d'un joueur n'a jamais démarré (Docker, gVisor) : le serveur est en faute.
    #[error("bac à sable du joueur {side} jamais démarré (code {code}), partie annulée")]
    Infra { side: Color, code: i32 },
    /// Le nœud qui jouait a échoué : arbitre, lancement, délai.
    #[error("nœud {node} : {message}")]
    Remote { node: String, message: String },
    /// Le nœud qui jouait a disparu en pleine partie : elle est rejouée ailleurs.
    #[error("nœud {0} perdu pendant la partie")]
    NodeLost(String),
}

/// Partie de tournoi : son temps par coup et sa session remplacent ceux du ladder.
pub struct Fixture {
    pub game: TournamentGame,
    pub time_ms: u64,
    /// Faux : un `no_show` annule la partie (`GameError::NoShow`) au lieu de la donner à l'adversaire.
    pub forfeit_counts: bool,
}

pub struct Sigterm(#[cfg(unix)] signal_hook::iterator::Signals);

impl Sigterm {
    #[cfg(unix)]
    pub fn listen() -> io::Result<Self> {
        signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM]).map(Self)
    }

    #[cfg(not(unix))]
    #[expect(clippy::unnecessary_wraps, reason = "même signature que sous Unix")]
    pub fn listen() -> io::Result<Self> {
        Ok(Self())
    }
}

/// Joue au plus une partie par slot, ladder et smoke tests confondus : pas de port ni de coeur partagé.
pub struct Arena {
    cfg: Config,
    placement: Arc<Placement>,
    local: Arc<Local>,
    /// Registre des nœuds, si `[nodes]` est configuré.
    nodes: Option<Arc<NodeHub>>,
    /// Levé par SIGTERM : plus de nouvelle partie, on finit celles en cours.
    draining: AtomicBool,
    smoke_tests: AtomicUsize,
    hub: Arc<Hub>,
}

impl Arena {
    pub fn new(cfg: Config, hub: Arc<Hub>) -> Self {
        let placement = Placement::new(&cfg);
        // L'état des nœuds (noms d'hôtes internes, erreurs) va à côté de la base, jamais dans data/,
        // que la page publie.
        let state = cfg.db.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
        let nodes = cfg
            .nodes
            .as_ref()
            .map(|nodes| NodeHub::new(Arc::clone(&placement), nodes.tokens.clone(), state));

        Self {
            placement,
            local: Arc::new(Local::new(cfg.exec.clone())),
            nodes,
            cfg,
            draining: AtomicBool::new(false),
            smoke_tests: AtomicUsize::new(0),
            hub,
        }
    }

    pub const fn config(&self) -> &Config {
        &self.cfg
    }

    pub const fn placement(&self) -> &Arc<Placement> {
        &self.placement
    }

    pub const fn nodes(&self) -> Option<&Arc<NodeHub>> {
        self.nodes.as_ref()
    }

    /// Qui joue sur ce slot : cette machine, ou le nœud qui le prête.
    fn executor(&self, slot: &SlotId) -> Arc<dyn Executor> {
        match &self.nodes {
            Some(nodes) if !slot.local() => nodes.executor(&slot.node),
            _ => Arc::clone(&self.local) as Arc<dyn Executor>,
        }
    }

    #[cfg(unix)]
    pub fn drain_on(self: &Arc<Self>, mut sigterm: Sigterm) {
        let arena = Arc::clone(self);
        std::thread::spawn(move || {
            for _ in sigterm.0.forever() {
                arena.drain();
            }
        });
    }

    #[cfg(not(unix))]
    #[expect(clippy::unused_self, reason = "même signature que sous Unix")]
    pub fn drain_on(self: &Arc<Self>, _: Sigterm) {}

    #[cfg_attr(not(unix), allow(dead_code))]
    pub fn drain(&self) {
        if !self.draining.swap(true, Ordering::SeqCst) {
            eprintln!("arrêt demandé : fin des parties en cours, aucune nouvelle");
            if let Some(nodes) = &self.nodes {
                nodes.drain();
            }
        }
    }

    pub fn draining(&self) -> bool {
        self.draining.load(Ordering::SeqCst)
    }

    /// Réserve la place d'un smoke test, refusée pendant l'arrêt. Compter avant de lire `draining` :
    /// la boucle, qui lit dans l'ordre inverse, ne peut pas sortir sans voir ce smoke test.
    pub fn hold(&self) -> Option<Hold<'_>> {
        self.smoke_tests.fetch_add(1, Ordering::SeqCst);
        let hold = Hold(self);
        (!self.draining()).then_some(hold)
    }

    pub fn smoke_tests(&self) -> usize {
        self.smoke_tests.load(Ordering::SeqCst)
    }

    /// Ramasse les restes d'un arrêt brutal : `cleanup` avec `{game}` vide.
    pub fn sweep(&self) {
        self.local.sweep();
    }

    /// Joue sur le slot de `lease` ; sans bail (smoke test), sur un slot du serveur principal.
    pub fn play(
        &self,
        lease: Option<&Lease>,
        red: &Team,
        black: &Team,
        opening: &[Move],
        fixture: Option<&Fixture>,
        live: bool,
    ) -> Result<GameRecord, GameError> {
        let cfg = &self.cfg;
        let time_ms = fixture.map_or(cfg.time_ms, |f| f.time_ms);
        // La classe ne compte que pour un smoke test : un slot du serveur principal du bon genre.
        let class = self.placement.class(red.multithread || black.multithread);
        let slot = self.placement.occupy(lease, class);

        let ts = now_rfc3339();
        let id = format!("{}-{}-{}", compact_timestamp(&ts), red.id, black.id);
        let entry = |team: &Team| Player {
            team: team.id.clone(),
            build: team.remote.as_ref().map_or_else(|| build_hash(team), Seat::build),
            anchor: team.anchor,
            remote: team.remote.is_some(),
        };
        let (red_entry, black_entry) = (entry(red), entry(black));
        let mut on_air = live.then(|| {
            self.hub.start(&hub::Start {
                id: &id,
                red: &red.id,
                black: &black.id,
                since: &ts,
                time_ms,
                opening,
                tournament: fixture.map(|f| &f.game),
            });
            OnAir {
                hub: Arc::clone(&self.hub),
                id: id.clone(),
                result: None,
            }
        });
        let out = Arc::new(LadderOutput {
            hub: on_air.as_ref().map(|o| Arc::clone(&o.hub)),
            id: id.clone(),
            logs: ByColor {
                red: cfg.player_log(&id, &red.id),
                black: cfg.player_log(&id, &black.id),
            },
        });
        let run = GameRun {
            id: &id,
            slot: slot.index(),
            time_ms,
            opening,
            red,
            black,
            timeout: cfg.game_timeout(time_ms),
        };
        let result = self.executor(slot.slot()).run(&run, out)?;
        if let Some(absent) = annulled_no_show(&result, fixture, red, black) {
            return Err(GameError::NoShow(absent.id.clone()));
        }
        if let Some(on_air) = &mut on_air {
            on_air.result = Some((result.winner, result.reason));
        }

        Ok(GameRecord {
            id,
            session: fixture.map_or_else(|| cfg.session.clone(), |f| f.game.id.clone()),
            ts,
            red: red_entry,
            black: black_entry,
            tournament: fixture.map(|f| f.game.clone()),
            node: (!slot.slot().local()).then(|| slot.slot().node.to_string()),
            result,
        })
    }
}

/// Le direct de la page et les journaux du serveur principal.
struct LadderOutput {
    hub: Option<Arc<Hub>>,
    id: String,
    logs: ByColor<PathBuf>,
}

impl GameOutput for LadderOutput {
    fn event(&self, event: RefereeEvent) {
        if let Some(hub) = &self.hub {
            hub.event(&self.id, event);
        }
    }

    fn log_path(&self, side: Color) -> PathBuf {
        self.logs[side].clone()
    }
}

struct OnAir {
    hub: Arc<Hub>,
    id: String,
    result: Option<(Color, Reason)>,
}

impl Drop for OnAir {
    fn drop(&mut self) {
        self.hub.end(&self.id, self.result);
    }
}

pub struct Hold<'a>(&'a Arena);

impl Drop for Hold<'_> {
    fn drop(&mut self) {
        self.0.smoke_tests.fetch_sub(1, Ordering::SeqCst);
    }
}

/// L'absent d'un `no_show` de tournoi qui n'a pas encore le droit de compter. Un jar figé a passé
/// le smoke test : s'il ne se connecte pas, le serveur (surchargé, trop chaud) est le premier suspect.
fn annulled_no_show<'t>(
    result: &GameResult,
    fixture: Option<&Fixture>,
    red: &'t Team,
    black: &'t Team,
) -> Option<&'t Team> {
    let counts = fixture.is_none_or(|f| f.forfeit_counts);
    (!counts && result.reason == Reason::NoShow).then(|| match result.winner.other() {
        Color::Red => red,
        Color::Black => black,
    })
}

fn build_hash(team: &Team) -> Option<String> {
    fs::read(team.build_file()?).ok().map(|bytes| short_hash(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_played_on_a_node_carry_its_id() {
        use crate::bridge::Socket;
        use crate::node::protocol::{FromNode, Outcome, PROTOCOL, ToNode};
        use crate::placement::PairClass;
        use axum::extract::ws::Message as Frame;

        let dir = std::env::temp_dir().join(format!("isolation-arena-node-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("logs")).unwrap();
        let cfg: Config = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\ndb = {db:?}\n[nodes]\n",
            out = dir.join("games.jsonl"),
            db = dir.join("state").join("ladder.db"),
        )
        .parse()
        .unwrap();
        let arena = Arena::new(cfg, Hub::new(|_, _, _| {}));
        let (socket, to_hub, mut from_hub) = Socket::pair();
        let hub = Arc::clone(arena.nodes().unwrap());
        std::thread::spawn(move || crate::node::hub::serve(&socket, &hub, "node-a"));
        let say = move |message: &FromNode| {
            to_hub
                .send(Frame::text(serde_json::to_string(message).unwrap()))
                .unwrap();
        };
        say(&FromNode::Hello {
            node: "node-a".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: PROTOCOL,
            arch: "x86_64".into(),
            slots: 1,
        });
        say(&FromNode::Ready { ready: true, why: None });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let lease = loop {
            if let Some(lease) = arena
                .placement()
                .try_lease(PairClass::Portable)
                .filter(|l| !l.slot().local())
            {
                break lease;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "le nœud n'a jamais offert de slot"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        let bot = |id: &str| Team {
            id: id.into(),
            cmd: crate::command::CommandTemplate::new(["run.sh", "--bot", "aleatoire"]),
            build: None,
            anchor: false,
            multithread: false,
            portable: true,
            remote: None,
        };
        let node = std::thread::spawn(move || {
            loop {
                let Some(Frame::Text(text)) = from_hub.blocking_recv() else {
                    continue;
                };
                if let Ok(ToNode::Start(start)) = serde_json::from_str(text.as_str()) {
                    let result = GameResult {
                        v: isolation_core::record::RECORD_VERSION,
                        time_ms: 100,
                        opening: vec![],
                        moves: vec![],
                        winner: Color::Black,
                        reason: Reason::Timeout,
                        invalid: isolation_core::record::InvalidCounts::default(),
                        duration_ms: 5,
                        move_ms: vec![],
                    };
                    say(&FromNode::Result {
                        game: start.game,
                        outcome: Outcome::Ok { result },
                        logs: ByColor::default(),
                    });
                    return;
                }
            }
        });
        let record = arena
            .play(Some(&lease), &bot("aleatoire"), &bot("niveau-1"), &[], None, false)
            .unwrap();
        node.join().unwrap();
        assert_eq!(record.node.as_deref(), Some("node-a"));
        assert!(dir.join("state").join("nodes.json").exists(), "état des nœuds : privé");
        assert!(
            !dir.join("nodes.json").exists(),
            "jamais dans data/, que la page publie"
        );

        let local = arena.placement().try_lease(PairClass::Heavy).unwrap();
        assert!(local.slot().local());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_sigterm_before_the_arena_exists_still_drains_it() {
        let sigterm = Sigterm::listen().unwrap();
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n".parse().unwrap();
        let arena = Arc::new(Arena::new(cfg, Hub::new(|_, _, _| {})));
        arena.drain_on(sigterm);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !arena.draining() {
            assert!(std::time::Instant::now() < deadline, "SIGTERM lost");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn draining_refuses_new_smoke_tests_and_keeps_counting_running_ones() {
        let cfg: Config = "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n".parse().unwrap();
        let arena = Arena::new(cfg, Hub::new(|_, _, _| {}));
        let running = arena.hold().unwrap();
        arena.drain();
        assert!(arena.draining());
        assert!(arena.hold().is_none());
        assert_eq!(arena.smoke_tests(), 1);
        drop(running);
        assert_eq!(arena.smoke_tests(), 0);
    }

    #[test]
    fn a_tournament_no_show_is_annulled_until_the_last_try() {
        let team = |id: &str| Team {
            id: id.into(),
            cmd: crate::command::CommandTemplate::default(),
            build: None,
            anchor: false,
            multithread: false,
            portable: false,
            remote: None,
        };
        let (red, black) = (team("eq01"), team("eq02"));
        let result = |winner, reason| GameResult {
            v: isolation_core::record::RECORD_VERSION,
            time_ms: 5000,
            opening: vec![],
            moves: vec![],
            winner,
            reason,
            invalid: isolation_core::record::InvalidCounts::default(),
            duration_ms: 10_000,
            move_ms: vec![],
        };
        let fixture = |forfeit_counts| Fixture {
            game: TournamentGame {
                id: "tournoi-2026-11-08".into(),
                match_id: "W1".into(),
                game: 1,
            },
            time_ms: 5000,
            forfeit_counts,
        };
        let absent = |r: &GameResult, f: Option<&Fixture>| annulled_no_show(r, f, &red, &black).map(|t| t.id.clone());
        let black_absent = result(Color::Red, Reason::NoShow);
        assert_eq!(absent(&black_absent, Some(&fixture(false))).as_deref(), Some("eq02"));
        assert_eq!(
            absent(&result(Color::Black, Reason::NoShow), Some(&fixture(false))).as_deref(),
            Some("eq01")
        );
        assert_eq!(absent(&black_absent, Some(&fixture(true))), None);
        assert_eq!(absent(&black_absent, None), None);
        assert_eq!(
            absent(&result(Color::Red, Reason::Timeout), Some(&fixture(false))),
            None
        );
    }
}
