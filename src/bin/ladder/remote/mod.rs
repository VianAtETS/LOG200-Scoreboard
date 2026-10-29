//! Équipes distantes : `isolation-remote` ouvre une WebSocket sur `/api/remote` (docs/formats.md) et
//! relaie le jar de l'équipe, qui tourne sur sa machine. Tant que la session est ouverte, l'équipe
//! est disponible ; le temps d'une partie, le ladder la relie à l'arbitre à la place d'un jar.

mod session;
mod stream;

use std::collections::HashMap;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use anyhow::Context;

use isolation::board::Color;
use isolation::remote::PATH;

use crate::config::{CommandTemplate, Config, Team};
use session::{Session, Start};

/// Le relais écrit encore son diagnostic quand l'arène le libère.
const FINISH_WAIT: Duration = Duration::from_secs(2);

pub struct Registry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    /// Dernier build déclaré par chaque équipe, gardé après sa déconnexion.
    builds: Mutex<HashMap<String, String>>,
    /// Réveille la boucle du ladder quand une équipe arrive.
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Registry {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            builds: Mutex::new(HashMap::new()),
            wake: Box::new(wake),
        })
    }

    /// Équipes disponibles pour une paire, par id.
    pub fn online(self: &Arc<Self>) -> Vec<Team> {
        let mut teams: Vec<Team> = lock(&self.sessions)
            .values()
            .filter(|session| session.available())
            .map(|session| Team {
                id: session.team.clone(),
                cmd: CommandTemplate::default(),
                build: None,
                anchor: false,
                multithread: false,
                remote: Some(Seat {
                    registry: Arc::clone(self),
                    team: session.team.clone(),
                }),
            })
            .collect();
        teams.sort_by(|a, b| a.id.cmp(&b.id));
        teams
    }

    /// Remplace une session au repos de la même équipe ; refusée pendant une partie.
    fn open(&self, session: &Arc<Session>, build: &str) -> Result<(), &'static str> {
        let mut sessions = lock(&self.sessions);
        if let Some(old) = sessions.get(&session.team) {
            if old.playing() {
                return Err("une partie de l'équipe est en cours : se reconnecter à la fin");
            }
            old.close("remplacée par une nouvelle connexion de l'équipe");
        }
        sessions.insert(session.team.clone(), Arc::clone(session));
        lock(&self.builds).insert(session.team.clone(), build.to_string());
        Ok(())
    }

    fn remove(&self, session: &Arc<Session>) {
        let mut sessions = lock(&self.sessions);
        if sessions.get(&session.team).is_some_and(|s| Arc::ptr_eq(s, session)) {
            sessions.remove(&session.team);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Place d'une équipe distante dans une partie.
#[derive(Clone)]
pub struct Seat {
    registry: Arc<Registry>,
    team: String,
}

pub struct GameSpec {
    pub id: String,
    pub side: Color,
    pub time_ms: u64,
    /// `connect` de `[remote]`, gabarit substitué.
    pub connect: String,
    pub log: PathBuf,
}

impl Seat {
    pub fn build(&self) -> Option<String> {
        lock(&self.registry.builds).get(&self.team).cloned()
    }

    /// Jamais d'erreur : sans session, personne ne se connecte et l'arbitre conclut `no_show`. Une
    /// équipe appariée ne peut donc pas éviter une partie en se déconnectant.
    pub fn sit(&self, game: GameSpec) -> Seated {
        let over = Arc::new(AtomicBool::new(false));
        let (done, finished) = mpsc::channel();
        let session = lock(&self.registry.sessions).get(&self.team).cloned();
        match session {
            Some(session) => session.start(Start {
                game,
                over: Arc::clone(&over),
                done,
            }),
            None => {
                let _ = fs::write(&game.log, "[relais] équipe déconnectée avant la partie\n");
            }
        }
        Seated { over, finished }
    }
}

/// Tenu par l'arène comme le processus d'un joueur : le libérer termine le relais de la partie.
pub struct Seated {
    over: Arc<AtomicBool>,
    finished: Receiver<()>,
}

impl Drop for Seated {
    fn drop(&mut self) {
        self.over.store(true, Ordering::Relaxed);
        let _ = self.finished.recv_timeout(FINISH_WAIT);
    }
}

/// Écoute `/api/remote` dans un thread, si `[remote]` est configuré.
pub fn serve(registry: &Arc<Registry>, cfg: &Config) -> anyhow::Result<()> {
    let (Some(remote), Some(submit)) = (&cfg.remote, &cfg.submit) else {
        return Ok(());
    };
    let listener = TcpListener::bind(&remote.listen)
        .with_context(|| format!("écoute des équipes distantes sur {}", remote.listen))?;
    eprintln!("équipes distantes : ws://{}{PATH}", remote.listen);
    listen(Arc::clone(registry), listener, submit.teams.clone(), cfg.time_ms);
    Ok(())
}

fn listen(registry: Arc<Registry>, listener: TcpListener, teams: PathBuf, time_ms: u64) {
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (registry, teams) = (Arc::clone(&registry), teams.clone());
            thread::spawn(move || session::run(stream, &registry, &teams, time_ms));
        }
    });
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::time::Instant;

    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::HeaderValue;
    use tungstenite::{Error as WsError, Message as Frame, WebSocket};

    use isolation::remote::{Message, PROTOCOL_VERSION};

    use super::*;
    use crate::submit::{TeamId, add_team, test_support::temp_dir};

    const BUILD: &str = "3fa9c2d1e0b4a7f6";

    struct Fixture {
        registry: Arc<Registry>,
        addr: String,
        token: String,
        dir: PathBuf,
    }

    fn fixture(name: &str) -> Fixture {
        let dir = temp_dir(&format!("remote-{name}"));
        let teams = dir.join("teams.toml");
        let token = add_team(&teams, "eq07".parse::<TeamId>().unwrap()).unwrap();
        let registry = Registry::new(|| {});
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        listen(Arc::clone(&registry), listener, teams, 500);
        Fixture {
            registry,
            addr,
            token,
            dir,
        }
    }

    fn connect(f: &Fixture, token: &str) -> Result<WebSocket<TcpStream>, WsError> {
        let stream = TcpStream::connect(&f.addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut request = format!("ws://{}{PATH}", f.addr).into_client_request().unwrap();
        let bearer = HeaderValue::from_str(&format!("Bearer {token}")).unwrap();
        request.headers_mut().insert("Authorization", bearer);
        tungstenite::client(request, stream)
            .map(|(ws, _)| ws)
            .map_err(|e| match e {
                tungstenite::HandshakeError::Failure(e) => e,
                tungstenite::HandshakeError::Interrupted(_) => unreachable!("flux bloquant"),
            })
    }

    fn send(ws: &mut WebSocket<TcpStream>, message: &Message) {
        ws.send(Frame::text(message.to_json())).unwrap();
    }

    /// Prochain message du protocole, ou la raison de la fermeture.
    fn recv(ws: &mut WebSocket<TcpStream>) -> Result<Message, String> {
        loop {
            match ws.read() {
                Ok(Frame::Text(text)) => return Ok(Message::from_json(text.as_str()).unwrap()),
                Ok(Frame::Close(frame)) => return Err(frame.map(|f| f.reason.to_string()).unwrap_or_default()),
                Ok(_) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    fn joined(f: &Fixture, build: &str) -> WebSocket<TcpStream> {
        let mut ws = connect(f, &f.token).unwrap();
        let hello = Message::Hello {
            v: PROTOCOL_VERSION,
            build: build.into(),
            client: "test".into(),
        };
        send(&mut ws, &hello);
        assert_eq!(
            recv(&mut ws),
            Ok(Message::Welcome {
                team: "eq07".into(),
                time_ms: 500
            })
        );
        ws
    }

    fn eventually(what: &str, check: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !check() {
            assert!(Instant::now() < deadline, "{what}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn spec(f: &Fixture, connect: String) -> GameSpec {
        GameSpec {
            id: "g1".into(),
            side: Color::Red,
            time_ms: 500,
            connect,
            log: f.dir.join("g1.log"),
        }
    }

    #[test]
    fn unknown_token_is_refused_before_the_upgrade() {
        let f = fixture("token");
        match connect(&f, "faux") {
            Err(WsError::Http(response)) => assert_eq!(response.status(), 401),
            other => panic!("401 attendu : {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn hello_must_declare_a_build() {
        let f = fixture("hello");
        let mut ws = connect(&f, &f.token).unwrap();
        let hello = Message::Hello {
            v: PROTOCOL_VERSION,
            build: "abc".into(),
            client: "test".into(),
        };
        send(&mut ws, &hello);
        assert!(recv(&mut ws).unwrap_err().contains("build invalide"));
        assert!(f.registry.online().is_empty());
    }

    #[test]
    fn a_game_is_relayed_line_by_line_until_the_referee_hangs_up() {
        let f = fixture("game");
        let mut ws = joined(&f, BUILD);
        eventually("équipe en ligne", || f.registry.online().len() == 1);
        let team = f.registry.online().remove(0);
        assert!(!team.is_bot(), "une équipe distante n'est pas un bot");
        let seat = team.remote.unwrap();
        assert_eq!(seat.build().as_deref(), Some(BUILD));

        let referee = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = format!("tcp:{}", referee.local_addr().unwrap());
        let seated = seat.sit(spec(&f, target));
        assert!(f.registry.online().is_empty(), "en partie, hors du tirage");
        assert_eq!(
            recv(&mut ws),
            Ok(Message::Game {
                id: "g1".into(),
                side: Color::Red,
                time_ms: 500
            })
        );
        send(&mut ws, &Message::Ready);
        let (mut player, _) = referee.accept().unwrap();
        player.write_all(b"0 2 12 12 500 0\n1 0 0 0\n").unwrap();
        for text in ["0 2 12 12 500 0", "1 0 0 0"] {
            assert_eq!(recv(&mut ws), Ok(Message::Line { text: text.into() }));
        }
        send(
            &mut ws,
            &Message::Line {
                text: "G1-G4-F9".into(),
            },
        );
        let mut reader = BufReader::new(player.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert_eq!(line, "G1-G4-F9\n");
        player.write_all(b"5 WIN G1-G4-F9\n").unwrap();
        drop((reader, player));
        assert_eq!(
            recv(&mut ws),
            Ok(Message::Line {
                text: "5 WIN G1-G4-F9".into()
            })
        );
        assert_eq!(recv(&mut ws), Ok(Message::End { id: "g1".into() }));
        drop(seated);

        let log = fs::read_to_string(f.dir.join("g1.log")).unwrap();
        assert!(log.contains("jar prêt en"), "{log}");
        eventually("de nouveau en ligne", || f.registry.online().len() == 1);
    }

    #[test]
    fn a_client_that_never_starts_its_jar_is_let_go() {
        let f = fixture("silent");
        let mut ws = joined(&f, BUILD);
        eventually("équipe en ligne", || f.registry.online().len() == 1);
        let seat = f.registry.online().remove(0).remote.unwrap();
        let seated = seat.sit(spec(&f, "tcp:127.0.0.1:9".into()));
        assert!(matches!(recv(&mut ws), Ok(Message::Game { .. })));
        drop(seated);
        assert!(recv(&mut ws).unwrap_err().contains("jamais connecté"));
        eventually("session retirée", || lock(&f.registry.sessions).is_empty());
        // Le build reste connu : une partie lancée après une déconnexion le garde.
        assert_eq!(seat.build().as_deref(), Some(BUILD));
        drop(seat.sit(spec(&f, "tcp:127.0.0.1:9".into())));
        let log = fs::read_to_string(f.dir.join("g1.log")).unwrap();
        assert!(log.contains("déconnectée"), "{log}");
    }

    #[test]
    fn a_new_connection_replaces_an_idle_session() {
        let f = fixture("replace");
        let mut first = joined(&f, BUILD);
        let _second = joined(&f, "0000000000000000");
        assert!(recv(&mut first).unwrap_err().contains("remplacée"));
        eventually("une seule session", || f.registry.online().len() == 1);
        let seat = f.registry.online().remove(0).remote.unwrap();
        assert_eq!(seat.build().as_deref(), Some("0000000000000000"));
    }
}
