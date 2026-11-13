//! The HTTP and WebSocket contract the teams' CI, `isolation-remote` and the page rely on, whatever
//! serves it.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use zip::write::SimpleFileOptions;

mod support;

const LADDER: &str = env!("CARGO_BIN_EXE_isolation-ladder");
const MAX_JAR_MB: u64 = 1;
const REGISTRY: &str = "jars/teams.toml";
/// Imported from the old `games.jsonl` at startup, served back by the replay.
const GAME: &str = concat!(
    r#"{"id":"g1","session":"api","ts":"2026-10-25T00:35:15.441Z","red":{"team":"a"},"black":{"team":"b"},"#,
    r#""v":1,"time_ms":500,"opening":[],"moves":[],"winner":"red","reason":"no_show","#,
    r#""invalid":{"red":0,"black":0},"duration_ms":10}"#,
    "\n"
);

struct Ladder {
    _process: support::Killed,
    dir: PathBuf,
    submit: String,
    remote: String,
    live: String,
    nodes: String,
}

impl Ladder {
    fn start(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("isolation-api-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("data")).unwrap();
        fs::write(dir.join("data/games.jsonl"), GAME).unwrap();
        fs::write(
            dir.join("ladder.toml"),
            format!(
                "referee = [\"unused\"]\ntime_ms = 1000\nsession = \"api\"\nteam_cmd = [\"unused\"]\n\
                 [submit]\nlisten = \"127.0.0.1:0\"\nmax_jar_mb = {MAX_JAR_MB}\n\
                 [remote]\nlisten = \"127.0.0.1:0\"\n\
                 [live]\nlisten = \"127.0.0.1:0\"\n\
                 [nodes]\nlisten = \"127.0.0.1:0\"\n"
            ),
        )
        .unwrap();
        let mut process = Command::new(LADDER)
            .arg("ladder.toml")
            .current_dir(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(process.stderr.take().unwrap());
        let [mut submit, mut remote, mut live, mut nodes] = [None, None, None, None];
        while submit.is_none() || remote.is_none() || live.is_none() || nodes.is_none() {
            let mut announced = String::new();
            assert_ne!(
                stderr.read_line(&mut announced).unwrap(),
                0,
                "ladder exited before listening"
            );
            let address = |path: &str| {
                let (_, rest) = announced.split_once("://")?;
                rest.trim_end().strip_suffix(path).map(str::to_string)
            };
            submit = submit.or_else(|| address("/api/submit"));
            remote = remote.or_else(|| address("/api/remote"));
            live = live.or_else(|| address("/api/live"));
            nodes = nodes.or_else(|| address("/api/node"));
        }
        thread::spawn(move || io::copy(&mut stderr, &mut io::sink()));
        Self {
            _process: support::Killed(process),
            dir,
            submit: submit.unwrap(),
            remote: remote.unwrap(),
            live: live.unwrap(),
            nodes: nodes.unwrap(),
        }
    }

    fn add_team(&self, team: &str, registry: &str) -> String {
        let output = Command::new(LADDER)
            .args(["add-team", team, "--teams", registry])
            .current_dir(&self.dir)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    /// Status code and first line of the body, what the teams' CI reads.
    fn request(&self, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> (u16, String) {
        Self::request_at(&self.submit, method, path, token, body)
    }

    fn request_at(address: &str, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> (u16, String) {
        let mut stream = TcpStream::connect(address).unwrap();
        let authorization = token.map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n",
            address,
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").unwrap();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        (status, body.lines().next().unwrap_or_default().to_string())
    }
}

fn jar() -> Vec<u8> {
    let mut jar = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    jar.start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
        .unwrap();
    jar.write_all(b"Manifest-Version: 1.0\r\nMain-Class: Client\r\n")
        .unwrap();
    jar.finish().unwrap().into_inner()
}

fn install(dir: &Path, team: &str, jar: &[u8]) {
    let team_dir = dir.join("jars").join(team);
    fs::create_dir_all(&team_dir).unwrap();
    fs::write(team_dir.join("current.jar"), jar).unwrap();
}

#[test]
fn submit_answers_with_the_status_and_reason_the_teams_ci_reads() {
    let ladder = Ladder::start("submit");
    let token = ladder.add_team("eq01", REGISTRY);
    let stranger = ladder.add_team("eq01", "elsewhere.toml");
    let jar = jar();
    install(&ladder.dir, "eq01", &jar);
    let too_large = vec![0; usize::try_from(MAX_JAR_MB << 20).unwrap() + 1];

    assert_eq!(ladder.request("GET", "/api/health", None, &[]), (200, "ok".into()));
    assert_eq!(
        ladder.request("GET", "/api/version", None, &[]),
        (200, env!("CARGO_PKG_VERSION").into())
    );
    assert_eq!(
        ladder.request("GET", "/api/submit", None, &[]),
        (404, "inconnu : seul PUT /api/submit existe".into())
    );
    assert_eq!(
        ladder.request("GET", "/api/elsewhere", None, &[]),
        (404, "inconnu : seul PUT /api/submit existe".into())
    );
    let unauthorized = (
        401,
        "token d'équipe absent ou inconnu (en-tête Authorization: Bearer <token>)".to_string(),
    );
    assert_eq!(ladder.request("PUT", "/api/submit", None, &jar), unauthorized);
    assert_eq!(
        ladder.request("PUT", "/api/submit", Some(&stranger), &jar),
        unauthorized
    );
    assert_eq!(
        ladder.request("PUT", "/api/submit", Some(&token), &jar[1..]),
        (400, "le corps de la requête n'est pas un jar".into())
    );
    assert_eq!(
        ladder.request("PUT", "/api/submit", Some(&token), &too_large),
        (413, format!("jar trop gros (max {MAX_JAR_MB} Mo)"))
    );
    for method in ["PUT", "POST"] {
        let (status, reason) = ladder.request(method, "/api/submit", Some(&token), &jar);
        assert_eq!(status, 200, "{method}: {reason}");
        assert!(reason.starts_with("déjà actif : "), "{method}: {reason}");
    }

    let (status, path) = ladder.request("PUT", "/api/submit?async", Some(&token), &jar);
    assert_eq!(status, 202, "{path}");
    assert!(path.starts_with("/api/submit/"), "{path}");
    let mut verdict = ladder.request("GET", &path, Some(&token), &[]);
    for _ in 0..200 {
        if verdict.0 != 202 {
            break;
        }
        thread::sleep(std::time::Duration::from_millis(50));
        verdict = ladder.request("GET", &path, Some(&token), &[]);
    }
    assert_eq!(verdict.0, 200, "{}", verdict.1);
    assert!(verdict.1.starts_with("déjà actif : "), "{}", verdict.1);
    assert_eq!(ladder.request("GET", &path, None, &[]), unauthorized);
    let unknown = (
        404,
        "soumission inconnue : verdict gardé une heure, perdu si le ladder redémarre".to_string(),
    );
    let other_team = ladder.add_team("eq02", REGISTRY);
    assert_eq!(ladder.request("GET", &path, Some(&other_team), &[]), unknown);
    assert_eq!(ladder.request("GET", "/api/submit/x", Some(&token), &[]), unknown);
}

#[test]
fn live_sends_a_snapshot_first() {
    let ladder = Ladder::start("live");
    let (mut socket, response) = tungstenite::connect(format!("ws://{}/api/live", ladder.live)).unwrap();
    assert_eq!(response.status(), 101);
    let snapshot: serde_json::Value = serde_json::from_str(socket.read().unwrap().to_text().unwrap()).unwrap();
    assert_eq!(snapshot["v"], 1);
    assert_eq!(snapshot["type"], "snapshot");
    assert_eq!(snapshot["games"], serde_json::json!([]));
}

#[test]
fn remote_accepts_only_a_team_token() {
    let ladder = Ladder::start("remote");
    let token = ladder.add_team("eq01", REGISTRY);
    let url = format!("ws://{}/api/remote", ladder.remote);
    let refused = tungstenite::connect(&url).unwrap_err();
    assert!(
        matches!(&refused, tungstenite::Error::Http(response) if response.status() == 401),
        "{refused}"
    );
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}")).unwrap(),
    );
    let (_, response) = tungstenite::connect(request).unwrap();
    assert_eq!(response.status(), 101);
}

#[test]
fn node_route_is_served_only_on_the_nodes_address() {
    let ladder = Ladder::start("nodes");
    let (status, _) = ladder.request("GET", "/api/node", None, b"");
    assert_eq!(status, 404, "jamais sur l'adresse publique, que le proxy relaie");
    let refused = tungstenite::connect(format!("ws://{}/api/node", ladder.nodes)).unwrap_err();
    assert!(
        matches!(&refused, tungstenite::Error::Http(response) if response.status() == 401),
        "{refused}"
    );
    let (status, _) = Ladder::request_at(&ladder.nodes, "GET", "/api/games", None, b"");
    assert_eq!(status, 404, "l'adresse des nœuds ne sert rien d'autre");
}

#[test]
fn replay_reads_one_game_by_id_and_lets_caches_keep_it() {
    let ladder = Ladder::start("replay");
    let mut stream = TcpStream::connect(&ladder.submit).unwrap();
    write!(
        stream,
        "GET /api/games/g1 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let head = head.to_ascii_lowercase();
    assert!(head.contains("content-type: application/json"), "{head}");
    assert!(
        head.contains("cache-control: public, max-age=31536000, immutable"),
        "{head}"
    );
    assert_eq!(format!("{body}\n"), GAME);

    assert_eq!(ladder.request("GET", "/api/games/absent", None, b"").0, 404);
    assert_eq!(
        ladder.request("GET", "/api/games?team=a", None, b""),
        (200, GAME.trim_end().to_owned())
    );
    assert_eq!(ladder.request("GET", "/api/games?team=a&before=g1", None, b"").1, "");
    assert_eq!(ladder.request("GET", "/api/games?limit=x", None, b"").0, 400);
}

#[test]
fn stats_answer_in_json_for_fifteen_seconds() {
    let ladder = Ladder::start("stats");
    let mut stream = TcpStream::connect(&ladder.submit).unwrap();
    write!(
        stream,
        "GET /api/stats/teams?session=api HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    let head = head.to_ascii_lowercase();
    assert!(head.contains("content-type: application/json"), "{head}");
    assert!(head.contains("cache-control: public, max-age=15"), "{head}");
    let teams: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(teams[0]["team"], "a");
    assert_eq!(teams[1]["losses"]["no_show"], 1);

    assert_eq!(ladder.request("GET", "/api/stats/head-to-head", None, b"").0, 200);
    assert_eq!(ladder.request("GET", "/api/stats/openings", None, b"").0, 200);
    assert_eq!(ladder.request("GET", "/api/stats/builds?team=a", None, b"").0, 200);
    assert_eq!(ladder.request("GET", "/api/stats/builds", None, b"").0, 400);
    for name in ["ratings", "move-times", "submissions"] {
        assert_eq!(
            ladder.request("GET", &format!("/api/stats/{name}?team=a"), None, b"").0,
            200
        );
        assert_eq!(ladder.request("GET", &format!("/api/stats/{name}"), None, b"").0, 400);
    }
    assert_eq!(ladder.request("GET", "/api/stats/inconnue", None, b"").0, 404);
}

#[test]
fn openapi_spec_lists_every_http_route() {
    let ladder = Ladder::start("openapi");
    let (status, body) = ladder.request("GET", "/api/openapi.json", None, b"");
    assert_eq!(status, 200);
    let spec: serde_json::Value = serde_json::from_str(&body).unwrap();
    let mut paths: Vec<&str> = spec["paths"].as_object().unwrap().keys().map(String::as_str).collect();
    paths.sort_unstable();
    assert_eq!(
        paths,
        [
            "/api/games",
            "/api/games/{id}",
            "/api/health",
            "/api/stats/builds",
            "/api/stats/head-to-head",
            "/api/stats/move-times",
            "/api/stats/openings",
            "/api/stats/ratings",
            "/api/stats/submissions",
            "/api/stats/teams",
            "/api/submit",
            "/api/submit/{id}",
            "/api/version",
        ]
    );
    assert!(spec["paths"]["/api/submit"]["post"].is_object());
    assert!(spec["components"]["schemas"]["GameRecord"].is_object());
}
