//! `GOLDEN_UPDATE=1 cargo test -p isolation-engine golden` rewrites the expected files.

use std::fs;
use std::path::PathBuf;

pub fn check(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/golden/{name}.txt"));
    if std::env::var_os("GOLDEN_UPDATE").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, actual).unwrap();
    }
    let expected = fs::read_to_string(&path).unwrap();
    for (line, (got, want)) in actual.lines().zip(expected.lines()).enumerate() {
        assert_eq!(got, want, "{name}.txt, line {}", line + 1);
    }
    assert_eq!(
        actual.lines().count(),
        expected.lines().count(),
        "{name}.txt, line count"
    );
}
