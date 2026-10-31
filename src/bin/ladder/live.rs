//! Parties en cours, publiées pour la page dans `live.json` (format dans docs/formats.md).

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::Serialize;

use isolation::record::{TournamentGame, now_rfc3339};

#[derive(Serialize)]
struct Running {
    red: String,
    black: String,
    since: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tournament: Option<TournamentGame>,
}

#[derive(Serialize)]
struct LiveFile<'a> {
    v: u32,
    games: &'a [Running],
}

pub struct Live {
    dir: PathBuf,
    games: Vec<Running>,
}

impl Live {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        let live = Self { dir, games: Vec::new() };
        live.write()?;
        Ok(live)
    }

    pub fn start(&mut self, red: &str, black: &str, tournament: Option<&TournamentGame>) -> io::Result<()> {
        self.games.push(Running {
            red: red.to_string(),
            black: black.to_string(),
            since: now_rfc3339(),
            tournament: tournament.cloned(),
        });
        self.write()
    }

    pub fn end(&mut self, red: &str, black: &str) -> io::Result<()> {
        self.games.retain(|g| g.red != red || g.black != black);
        self.write()
    }

    fn write(&self) -> io::Result<()> {
        let tmp = self.dir.join("live.json.tmp");
        fs::write(
            &tmp,
            serde_json::to_vec(&LiveFile {
                v: 1,
                games: &self.games,
            })?,
        )?;
        fs::rename(tmp, self.dir.join("live.json"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::submit::test_support::temp_dir;

    fn read(dir: &std::path::Path) -> Value {
        serde_json::from_str(&fs::read_to_string(dir.join("live.json")).unwrap()).unwrap()
    }

    fn teams(file: &Value) -> Vec<(&str, &str)> {
        file["games"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| (g["red"].as_str().unwrap(), g["black"].as_str().unwrap()))
            .collect()
    }

    #[test]
    fn starts_empty_and_tracks_games() {
        let dir = temp_dir("live-tracks");
        let mut live = Live::new(dir.clone()).unwrap();
        assert_eq!(read(&dir)["v"], 1);
        assert!(teams(&read(&dir)).is_empty());

        live.start("a", "b", None).unwrap();
        live.start("c", "d", None).unwrap();
        assert_eq!(teams(&read(&dir)), [("a", "b"), ("c", "d")]);
        assert!(read(&dir)["games"][0]["since"].as_str().unwrap().ends_with('Z'));

        live.end("a", "b").unwrap();
        live.start("b", "a", None).unwrap();
        assert_eq!(teams(&read(&dir)), [("c", "d"), ("b", "a")]);
        assert!(!dir.join("live.json.tmp").exists());
    }

    #[test]
    fn tournament_games_carry_their_match() {
        let dir = temp_dir("live-tournament");
        let mut live = Live::new(dir.clone()).unwrap();
        live.start("a", "b", None).unwrap();
        let game = TournamentGame {
            id: "tournoi-2026-11-08".into(),
            match_id: "W1".into(),
            game: 2,
        };
        live.start("c", "d", Some(&game)).unwrap();
        let file = read(&dir);
        assert!(file["games"][0].get("tournament").is_none());
        assert_eq!(file["games"][1]["tournament"]["match"], "W1");
    }

    #[test]
    fn restart_clears_a_stale_file() {
        let dir = temp_dir("live-restart");
        Live::new(dir.clone()).unwrap().start("a", "b", None).unwrap();
        Live::new(dir.clone()).unwrap();
        assert!(teams(&read(&dir)).is_empty());
    }
}
