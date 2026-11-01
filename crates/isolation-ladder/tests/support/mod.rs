use std::path::PathBuf;
use std::process::Command;

/// `CARGO_BIN_EXE_*` only covers this package's binaries: the others are built on demand.
pub fn binary(package: &str) -> PathBuf {
    let output = Command::new(env!("CARGO"))
        .args([
            "build",
            "--quiet",
            "--message-format=json",
            "--package",
            package,
            "--bin",
            package,
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .unwrap()
}
