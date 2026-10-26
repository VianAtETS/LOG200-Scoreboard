//! Fichiers en ajout seul : parties jouées et soumissions.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::ops::Range;
use std::path::Path;

use serde::Serialize;

/// Une ligne part en une seule écriture suivie d'un `sync` : un lecteur concurrent voit au pire
/// une dernière ligne incomplète, qu'il ignore. Renvoie les octets écrits (un seul écrivain par fichier).
pub fn append_jsonl(path: &Path, value: &impl Serialize) -> io::Result<Range<u64>> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let start = file.metadata()?.len();
    file.write_all(&line)?;
    file.sync_data()?;
    Ok(start..start + line.len() as u64)
}

/// Horodatage RFC 3339 sans `:` ni `-`, utilisable dans un nom de fichier.
pub fn compact_timestamp(rfc3339: &str) -> String {
    rfc3339.replace([':', '-'], "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_accumulate() {
        let path = std::env::temp_dir().join(format!("isolation-journal-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert_eq!(append_jsonl(&path, &[1]).unwrap(), 0..4);
        assert_eq!(append_jsonl(&path, &"deux").unwrap(), 4..11);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[1]\n\"deux\"\n");
        assert_eq!(compact_timestamp("2026-10-24T12:00:00.000Z"), "20261024T120000.000Z");
    }
}
