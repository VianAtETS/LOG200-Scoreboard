use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use isolation::record::now_rfc3339;

use super::write_atomic;
use crate::config::Config;
use crate::journal::compact_timestamp;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Request {
    Start,
    Resume {
        id: String,
        #[serde(rename = "match")]
        match_id: String,
    },
}

static SENT: AtomicUsize = AtomicUsize::new(0);

pub fn send(cfg: &Config, request: &Request) -> anyhow::Result<PathBuf> {
    let dir = dir(cfg).context("[submit] absent : pas de dossier jars")?;
    let n = SENT.fetch_add(1, Ordering::Relaxed);
    let name = format!(
        "{}-{}-{n:04}.json",
        compact_timestamp(&now_rfc3339()),
        std::process::id()
    );
    let path = dir.join(name);
    write_atomic(&path, &serde_json::to_vec(request)?).with_context(|| path.display().to_string())?;
    Ok(path)
}

/// Requêtes en attente, les plus anciennes d'abord ; une requête illisible est jetée.
pub fn take(cfg: &Config) -> Vec<Request> {
    let Some(Ok(entries)) = dir(cfg).map(fs::read_dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let request = fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| Ok(serde_json::from_slice(&bytes)?));
            let _ = fs::remove_file(&path);
            request
                .map_err(|e| eprintln!("requête admin {} ignorée : {e}", path.display()))
                .ok()
        })
        .collect()
}

fn dir(cfg: &Config) -> Option<PathBuf> {
    cfg.submit.as_ref().map(|s| s.jars.join("admin"))
}

#[cfg(test)]
mod tests {
    use super::super::draw;
    use super::*;

    #[test]
    fn requests_are_taken_once_in_order() {
        let (root, cfg) = draw::tests::setup(&["solo"]);
        let resume = Request::Resume {
            id: "tournoi-2026-11-08".into(),
            match_id: "W3".into(),
        };
        send(&cfg, &Request::Start).unwrap();
        let path = send(&cfg, &resume).unwrap();
        fs::write(path.with_file_name("zzz.json"), "{").unwrap();
        assert_eq!(take(&cfg), [Request::Start, resume]);
        assert!(take(&cfg).is_empty());
        assert_eq!(fs::read_dir(dir(&cfg).unwrap()).unwrap().count(), 0);
        let _ = fs::remove_dir_all(root);
    }
}
