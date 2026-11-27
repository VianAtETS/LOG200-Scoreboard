//! Ouverture de la base, avec import unique des anciens `games.jsonl` et `submissions.jsonl`.

use std::fs;
use std::path::Path;

use anyhow::Context;

use crate::config::Config;
use crate::db::Db;

pub fn open_db(cfg: &Config) -> anyhow::Result<Db> {
    let path = &cfg.db;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    }
    let db = Db::open(path).with_context(|| path.display().to_string())?;
    migrate(&cfg.out, db.is_empty()?, || db.import_jsonl(&cfg.out), "parties", path)?;
    if let Some(submit) = &cfg.submit {
        let old = submit.jars.join("submissions.jsonl");
        migrate(
            &old,
            !db.has_submissions()?,
            || db.import_submissions(&old),
            "soumissions",
            path,
        )?;
    }
    Ok(db)
}

/// Importe `old` une seule fois, tant que sa table est vide, puis le renomme en `.migrated`.
fn migrate(
    old: &Path,
    empty: bool,
    import: impl FnOnce() -> anyhow::Result<usize>,
    what: &str,
    db: &Path,
) -> anyhow::Result<()> {
    if !old.exists() {
        return Ok(());
    }
    if !empty {
        eprintln!(
            "ATTENTION : {} ignoré, {} contient déjà des {what}",
            old.display(),
            db.display()
        );
        return Ok(());
    }
    let count = import().with_context(|| old.display().to_string())?;
    let done = old.with_extension("jsonl.migrated");
    fs::rename(old, &done).with_context(|| done.display().to_string())?;
    eprintln!("{count} {what} importées de {} dans {}", old.display(), db.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn the_old_games_file_is_imported_once_then_renamed() {
        let dir = std::env::temp_dir().join(format!("isolation-migrate-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("data")).unwrap();
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\ndb = {db:?}\n",
            out = dir.join("data/games.jsonl"),
            db = dir.join("state/ladder.db"),
        );
        let cfg: Config = toml.parse().unwrap();
        let games = [
            db::tests::game("g1", "a", "b", game_kit::Color::Red),
            db::tests::game("g2", "b", "a", game_kit::Color::Black),
        ];
        let text: String = games.iter().map(|g| serde_json::to_string(g).unwrap() + "\n").collect();
        fs::write(&cfg.out, text).unwrap();

        assert_eq!(open_db(&cfg).unwrap().games().unwrap(), games);
        assert!(!cfg.out.exists());
        assert!(dir.join("data/games.jsonl.migrated").exists());

        fs::write(&cfg.out, serde_json::to_string(&games[0]).unwrap() + "\n").unwrap();
        assert_eq!(open_db(&cfg).unwrap().games().unwrap().len(), 2);
        assert!(cfg.out.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_old_submission_log_is_imported_once_then_renamed() {
        let dir = std::env::temp_dir().join(format!("isolation-migrate-submissions-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("jars")).unwrap();
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\ndb = {db:?}\n[submit]\njars = {jars:?}\n",
            out = dir.join("data/games.jsonl"),
            db = dir.join("state/ladder.db"),
            jars = dir.join("jars"),
        );
        let cfg: Config = toml.parse().unwrap();
        let log = dir.join("jars/submissions.jsonl");
        let line =
            r#"{"ts":"2026-11-01T10:00:00.000Z","team":"eq07","hash":"abc","verdict":"rejected","detail":"no_show"}"#;
        fs::write(&log, format!("{line}\n{{\"ts\": \"coupé\n")).unwrap();

        let db = open_db(&cfg).unwrap();
        assert!(db.has_submissions().unwrap());
        assert!(!log.exists());
        assert!(dir.join("jars/submissions.jsonl.migrated").exists());

        fs::write(&log, format!("{line}\n")).unwrap();
        drop(db);
        open_db(&cfg).unwrap();
        assert!(log.exists());
        let count: i64 = rusqlite::Connection::open(&cfg.db)
            .unwrap()
            .query_row("SELECT count(*) FROM submissions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let _ = fs::remove_dir_all(&dir);
    }
}
