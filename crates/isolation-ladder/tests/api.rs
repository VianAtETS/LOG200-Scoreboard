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

struct Ladder {
    _process: support::Killed,
    dir: PathBuf,
    submit: String,
    remote: String,
    live: String,
}

impl Ladder {
    fn start(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("isolation-api-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("ladder.toml"),
            format!(
                "referee = [\"unused\"]\ntime_ms = 1000\nsession = \"api\"\nteam_cmd = [\"unused\"]\n\
                 [submit]\nlisten = \"127.0.0.1:0\"\nmax_jar_mb = {MAX_JAR_MB}\n\
                 [remote]\nlisten = \"127.0.0.1:0\"\n\
                 [live]\nlisten = \"127.0.0.1:0\"\n"
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
        let [mut submit, mut remote, mut live] = [None, None, None];
        while submit.is_none() || remote.is_none() || live.is_none() {
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
        }
        thread::spawn(move || io::copy(&mut stderr, &mut io::sink()));
        Self {
            _process: support::Killed(process),
            dir,
            submit: submit.unwrap(),
            remote: remote.unwrap(),
            live: live.unwrap(),
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
        let mut stream = TcpStream::connect(&self.submit).unwrap();
        let authorization = token.map_or_else(String::new, |t| format!("Authorization: Bearer {t}\r\n"));
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\n{authorization}Content-Length: {}\r\nConnection: close\r\n\r\n",
            self.submit,
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
