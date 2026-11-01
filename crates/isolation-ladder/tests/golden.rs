//! `GOLDEN_UPDATE=1 cargo test -p isolation-ladder --test golden` rewrites the expected files.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIG: &str = r#"referee = ["unused"]
port = 9999
time_ms = 1000
session = "automne-2026"
out = "data/games.jsonl"
team_cmd = ["unused"]
[tournament]
"#;

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                found.insert(name, fs::read(&path).unwrap());
            }
        }
    }
    found
}

#[test]
fn replaying_history_publishes_the_same_files() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden");
    let dir = std::env::temp_dir().join(format!("isolation-golden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::copy(fixtures.join("games.jsonl"), dir.join("data/games.jsonl")).unwrap();
    fs::write(dir.join("ladder.toml"), CONFIG).unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_isolation-ladder"))
        .args(["ladder.toml", "--rounds", "0"])
        .current_dir(&dir)
        .status()
        .unwrap();
    assert!(status.success());

    let mut published = files(&dir.join("data"));
    published.remove("games.jsonl");
    let expected = fixtures.join("expected");
    if std::env::var_os("GOLDEN_UPDATE").is_some() {
        let _ = fs::remove_dir_all(&expected);
        for (name, bytes) in &published {
            let path = expected.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
    }
    let wanted = files(&expected);
    assert_eq!(
        published.keys().collect::<Vec<_>>(),
        wanted.keys().collect::<Vec<_>>(),
        "published files"
    );
    for (name, bytes) in &published {
        assert!(
            bytes == &wanted[name],
            "{name} differs from tests/fixtures/golden/expected/{name}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}
