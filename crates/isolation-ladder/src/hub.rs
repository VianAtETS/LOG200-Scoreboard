//! Parties en cours, diffusées coup par coup aux spectateurs (protocole de `/api/live` dans
//! docs/formats.md). Tout reste en mémoire. Une partie n'attend jamais un spectateur : celui dont la
//! file est pleine est lâché.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use isolation_core::board::{Color, Move};
use isolation_core::record::{Reason, RefereeEvent, TournamentGame};
use isolation_core::rules::Replay;

pub const LIVE_VERSION: u32 = 1;
const QUEUE: usize = 64;
const SHUTDOWN_WAIT: Duration = Duration::from_secs(1);

type OnStart = Box<dyn Fn(&str, &str, &str) + Send + Sync>;

pub struct Hub {
    state: Mutex<State>,
    watching: AtomicUsize,
    on_start: OnStart,
}

#[derive(Default)]
struct State {
    games: Vec<Running>,
    spectators: Vec<SyncSender<Arc<str>>>,
    closed: bool,
}

struct Running {
    game: Game,
    replay: Replay,
    turn_start: Instant,
    broken: bool,
}

#[derive(Serialize)]
struct Game {
    id: String,
    red: String,
    black: String,
    since: String,
    time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    tournament: Option<TournamentGame>,
    opening: Vec<String>,
    moves: Vec<String>,
    move_ms: Vec<u64>,
}

#[derive(Serialize)]
struct Shown<'a> {
    #[serde(flatten)]
    game: &'a Game,
    turn_ms: u64,
}

impl<'a> From<&'a Running> for Shown<'a> {
    fn from(running: &'a Running) -> Self {
        Self {
            game: &running.game,
            turn_ms: u64::try_from(running.turn_start.elapsed().as_millis()).unwrap_or(u64::MAX),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Out<'a> {
    Snapshot {
        games: Vec<Shown<'a>>,
    },
    Started {
        game: Shown<'a>,
    },
    Move {
        id: &'a str,
        n: usize,
        #[serde(rename = "move")]
        mv: &'a str,
        ms: u64,
    },
    Ended {
        id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        winner: Option<Color>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<Reason>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        cancelled: bool,
    },
}

#[derive(Serialize)]
struct Envelope<'a> {
    v: u32,
    #[serde(flatten)]
    message: Out<'a>,
}

fn encode(message: Out) -> Arc<str> {
    serde_json::to_string(&Envelope {
        v: LIVE_VERSION,
        message,
    })
    .unwrap_or_default()
    .into()
}

pub struct Start<'a> {
    pub id: &'a str,
    pub red: &'a str,
    pub black: &'a str,
    pub since: &'a str,
    pub time_ms: u64,
    pub opening: &'a [Move],
    pub tournament: Option<&'a TournamentGame>,
}

pub struct Subscription {
    pub snapshot: Arc<str>,
    pub events: Receiver<Arc<str>>,
    _watching: Watching,
}

struct Watching(Arc<Hub>);

impl Drop for Watching {
    fn drop(&mut self) {
        self.0.watching.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Hub {
    pub fn new(on_start: impl Fn(&str, &str, &str) + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            watching: AtomicUsize::new(0),
            on_start: Box::new(on_start),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn start(&self, start: &Start) {
        let replay = Replay::from_moves(start.opening.iter().copied());
        let running = Running {
            game: Game {
                id: start.id.to_string(),
                red: start.red.to_string(),
                black: start.black.to_string(),
                since: start.since.to_string(),
                time_ms: start.time_ms,
                tournament: start.tournament.cloned(),
                opening: start.opening.iter().map(Move::to_string).collect(),
                moves: Vec::new(),
                move_ms: Vec::new(),
            },
            broken: replay.is_err(),
            replay: replay.unwrap_or_default(),
            turn_start: Instant::now(),
        };
        {
            let mut state = self.lock();
            let message = encode(Out::Started {
                game: Shown::from(&running),
            });
            state.games.push(running);
            state.publish(&message);
        }
        (self.on_start)(start.id, start.red, start.black);
    }

    pub fn event(&self, id: &str, event: RefereeEvent) {
        let mut state = self.lock();
        let Some(running) = state.games.iter_mut().find(|r| r.game.id == id) else {
            return;
        };
        let (mv, ms) = match event {
            RefereeEvent::Ready => {
                running.turn_start = Instant::now();
                return;
            }
            RefereeEvent::Move { mv, ms } => (mv, ms),
        };
        if running.broken {
            return;
        }
        let index = running.game.opening.len() + running.game.moves.len();
        let played = mv
            .parse::<Move>()
            .map_err(|e| e.to_string())
            .and_then(|parsed| running.replay.play(index, parsed).map_err(|e| e.to_string()));
        if let Err(e) = played {
            eprintln!("   [{id}] direct interrompu : {e}");
            running.broken = true;
            return;
        }
        running.turn_start = Instant::now();
        let n = running.game.moves.len();
        let message = encode(Out::Move { id, n, mv: &mv, ms });
        running.game.moves.push(mv);
        running.game.move_ms.push(ms);
        state.publish(&message);
    }

    pub fn end(&self, id: &str, result: Option<(Color, Reason)>) {
        let mut state = self.lock();
        let before = state.games.len();
        state.games.retain(|r| r.game.id != id);
        if state.games.len() == before {
            return;
        }
        let message = encode(Out::Ended {
            id,
            winner: result.map(|(winner, _)| winner),
            reason: result.map(|(_, reason)| reason),
            cancelled: result.is_none(),
        });
        state.publish(&message);
    }

    pub fn subscribe(self: &Arc<Self>) -> Option<Subscription> {
        let mut state = self.lock();
        if state.closed {
            return None;
        }
        let snapshot = encode(Out::Snapshot {
            games: state.games.iter().map(Shown::from).collect(),
        });
        let (tx, events) = mpsc::sync_channel(QUEUE);
        state.spectators.push(tx);
        self.watching.fetch_add(1, Ordering::SeqCst);
        Some(Subscription {
            snapshot,
            events,
            _watching: Watching(Arc::clone(self)),
        })
    }

    pub fn shutdown(&self) {
        {
            let mut state = self.lock();
            state.closed = true;
            state.spectators.clear();
        }
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        while self.watching.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl State {
    fn publish(&mut self, message: &Arc<str>) {
        self.spectators.retain(|tx| tx.try_send(Arc::clone(message)).is_ok());
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    fn quiet() -> Arc<Hub> {
        Hub::new(|_, _, _| {})
    }

    fn start(hub: &Hub, id: &str, opening: &[Move]) {
        hub.start(&Start {
            id,
            red: "a",
            black: "b",
            since: "2026-11-01T12:00:00.000Z",
            time_ms: 5000,
            opening,
            tournament: None,
        });
    }

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn mv(text: &str) -> RefereeEvent {
        RefereeEvent::Move {
            mv: text.into(),
            ms: 12,
        }
    }

    #[test]
    fn a_spectator_gets_the_state_then_every_change_once() {
        let hub = quiet();
        start(&hub, "g1", &[]);
        hub.event("g1", mv("G1-G4-F9"));
        let watcher = hub.subscribe().unwrap();
        let snapshot = json(&watcher.snapshot);
        assert_eq!(snapshot["v"], 1);
        assert_eq!(snapshot["type"], "snapshot");
        assert_eq!(snapshot["games"][0]["moves"], serde_json::json!(["G1-G4-F9"]));
        assert_eq!(snapshot["games"][0]["move_ms"], serde_json::json!([12]));
        assert!(snapshot["games"][0]["turn_ms"].is_u64());

        hub.event("g1", mv("F12-F10-G7"));
        hub.end("g1", Some((Color::Red, Reason::Timeout)));
        let next = json(&watcher.events.try_recv().unwrap());
        assert_eq!((next["type"].as_str(), next["n"].as_u64()), (Some("move"), Some(1)));
        assert_eq!(next["move"], "F12-F10-G7");
        let ended = json(&watcher.events.try_recv().unwrap());
        assert_eq!(ended["winner"], "red");
        assert_eq!(ended["reason"], "timeout");
        assert!(ended.get("cancelled").is_none());
        assert!(watcher.events.try_recv().is_err());
    }

    #[test]
    fn moves_after_the_opening_are_checked_and_an_illegal_one_cuts_the_game() {
        let hub = quiet();
        let opening: Vec<Move> = ["G1-G4-F9", "F12-F10-G7"].iter().map(|m| m.parse().unwrap()).collect();
        start(&hub, "g1", &opening);
        let watcher = hub.subscribe().unwrap();
        hub.event("g1", mv("G1-G2-A1"));
        hub.event("g1", mv("pas un coup"));
        hub.event("g1", mv("F10-F11-A2"));
        assert!(watcher.events.try_recv().is_err(), "coup illégal : rien n'est relayé");
        hub.end("g1", None);
        let ended = json(&watcher.events.try_recv().unwrap());
        assert_eq!(ended["cancelled"], true);
        assert!(ended.get("winner").is_none());
    }

    #[test]
    fn a_full_queue_drops_the_spectator_without_blocking() {
        let hub = quiet();
        let slow = hub.subscribe().unwrap();
        for i in 0..=QUEUE {
            start(&hub, &format!("g{i}"), &[]);
        }
        assert!(hub.lock().spectators.is_empty());
        assert_eq!(slow.events.try_iter().count(), QUEUE);
    }

    #[test]
    fn starts_are_reported_and_unknown_games_ignored() {
        let (tx, rx) = mpsc::channel();
        let hub = Hub::new(move |id, red, black| tx.send(format!("{id} {red} {black}")).unwrap());
        start(&hub, "g1", &[]);
        assert_eq!(rx.try_recv().unwrap(), "g1 a b");
        hub.event("autre", mv("G1-G4-F9"));
        hub.end("autre", None);
        let watcher = hub.subscribe().unwrap();
        assert!(watcher.events.try_recv().is_err());
    }

    #[test]
    fn shutdown_closes_every_queue_and_refuses_newcomers() {
        let hub = quiet();
        let watcher = hub.subscribe().unwrap();
        let waiter = thread::spawn(move || {
            let gone = watcher.events.recv().is_err();
            drop(watcher);
            gone
        });
        hub.shutdown();
        assert!(waiter.join().unwrap());
        assert_eq!(hub.watching.load(Ordering::SeqCst), 0);
        assert!(hub.subscribe().is_none());
    }
}
