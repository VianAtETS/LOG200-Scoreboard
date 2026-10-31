//! Un tournoi complet joué par le ladder compilé, sans bac à sable : arbitre et bots aléatoires.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const LADDER: &str = env!("CARGO_BIN_EXE_isolation-ladder");

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn read(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn a_started_tournament_is_played_to_its_winner() {
    let dir = std::env::temp_dir().join(format!("isolation-e2e-tournament-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let config = dir.join("ladder.toml");
    let teams = dir.join("jars/teams.toml");
    fs::write(
        &config,
        format!(
            "referee = [{referee:?}, \"--port\", \"{{port}}\", \"--time-ms\", \"{{time_ms}}\", \"--opening\", \"{{opening}}\"]\n\
             port = {port}\ntime_ms = 100\nsession = \"e2e\"\nout = {out:?}\n\
             team_cmd = [{bot:?}, \"--strategy\", \"random\", \"--port\", \"{{port}}\"]\n\
             [submit]\nlisten = \"127.0.0.1:0\"\nteams = {teams:?}\njars = {jars:?}\n\
             [tournament]\ntime_ms = 100\n",
            referee = env!("CARGO_BIN_EXE_isolation-referee"),
            bot = env!("CARGO_BIN_EXE_isolation-bot"),
            port = free_port(),
            out = dir.join("data/games.jsonl"),
            jars = dir.join("jars"),
        ),
    )
    .unwrap();
    for team in ["eq01", "eq02", "eq03", "eq04"] {
        let added = Command::new(LADDER)
            .args(["add-team", team, "--teams"])
            .arg(&teams)
            .output()
            .unwrap();
        assert!(added.status.success());
        fs::create_dir_all(dir.join("jars").join(team)).unwrap();
        fs::write(dir.join("jars").join(team).join("current.jar"), team).unwrap();
    }

    let log = fs::File::create(dir.join("ladder.log")).unwrap();
    let _ladder = Killed(
        Command::new(LADDER)
            .arg(&config)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let request = Command::new(LADDER)
        .args(["tournament", "start"])
        .arg(&config)
        .output()
        .unwrap();
    assert!(request.status.success(), "{}", String::from_utf8_lossy(&request.stderr));

    let index = dir.join("data/tournaments/index.json");
    let deadline = Instant::now() + Duration::from_mins(2);
    let winner = loop {
        if let Some(winner) = read(&index).and_then(|i| i["tournaments"][0]["winner"].as_str().map(String::from)) {
            break winner;
        }
        assert!(
            Instant::now() < deadline,
            "pas de vainqueur : {}",
            fs::read_to_string(dir.join("ladder.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(200));
    };
    let id = read(&index).unwrap()["tournaments"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let view = read(&dir.join(format!("data/tournaments/{id}.json"))).unwrap();
    assert_eq!(view["winner"], winner.as_str());
    assert_eq!(view["places"].as_array().unwrap().len(), 4);
    assert!(!dir.join("jars/tournaments").join(&id).exists());
    let games = fs::read_to_string(dir.join("data/games.jsonl")).unwrap();
    assert!(games.lines().filter(|l| l.contains("\"tournament\"")).count() >= 7);
}
