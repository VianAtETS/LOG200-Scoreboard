//! Startup flag controls only the optional archive presentation metadata.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::process::Command;

#[test]
fn archive_mode_survives_startup_and_can_be_disabled_again() {
    let dir = std::env::temp_dir().join(format!("isolation-archive-startup-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let config = dir.join("ladder.toml");
    fs::write(
        &config,
        format!(
            "referee = [\"unused\"]\ntime_ms = 1000\nsession = \"test\"\nout = {:?}\ndb = {:?}\n",
            dir.join("data/games.jsonl"),
            dir.join("state/ladder.db")
        ),
    )
    .unwrap();
    for (flags, enabled) in [
        (vec![], false),
        (vec!["--archives"], true),
        (vec!["--archives=false"], false),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_isolation-ladder"))
            .arg(&config)
            .args(["--rounds", "0"])
            .args(flags)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        let summary: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("data/summary.json")).unwrap()).unwrap();
        assert_eq!(
            summary
                .get("archives")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            enabled
        );
        assert_eq!(summary["scopes"][""]["games"], 0);
    }
    fs::remove_dir_all(dir).unwrap();
}
