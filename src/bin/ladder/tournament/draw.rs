use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use isolation::digest::short_hash;
use isolation::record::now_rfc3339;

use crate::config::{Config, Team};
use crate::standings::{START, Standings};
use crate::submit::{self, TeamId};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Entrant {
    pub team: String,
    pub build: String,
    pub rating: f64,
    pub multithread: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Draw {
    pub v: u32,
    pub id: String,
    pub ts: String,
    pub session: String,
    pub time_ms: u64,
    pub seed: u64,
    /// Meilleure tête de série d'abord.
    pub entrants: Vec<Entrant>,
}

pub fn tournament_id(date: Date, taken: impl Fn(&str) -> bool) -> String {
    let base = format!("tournoi-{date}");
    let mut id = base.clone();
    let mut n = 1;
    while taken(&id) {
        n += 1;
        id = format!("{base}-{n}");
    }
    id
}

/// Sans partie dans la session : 1500, après les équipes classées à 1500.
pub fn seeding(mut candidates: Vec<(String, Option<(f64, usize)>)>) -> Vec<(String, f64)> {
    let key = |s: &Option<(f64, usize)>| s.unwrap_or((START, usize::MAX));
    candidates.sort_by(|(a, sa), (b, sb)| {
        let ((ra, oa), (rb, ob)) = (key(sa), key(sb));
        rb.total_cmp(&ra).then(oa.cmp(&ob)).then(a.cmp(b))
    });
    candidates.into_iter().map(|(team, s)| (team, key(&s).0)).collect()
}

/// `None` : moins de `min_teams` équipes avec un jar.
pub fn draw(cfg: &Config, standings: &Standings, id: &str, seed: u64) -> anyhow::Result<Option<Draw>> {
    let settings = cfg.tournament.clone().unwrap_or_default();
    let teams: HashMap<String, Team> = submit::active_teams(cfg)?
        .into_iter()
        .map(|t| (t.id.clone(), t))
        .collect();
    if teams.len() < settings.min_teams {
        return Ok(None);
    }
    let frozen = frozen_dir(cfg, id).context("[submit] absent")?;
    fs::create_dir_all(&frozen).with_context(|| frozen.display().to_string())?;
    let seeds = seeding(
        teams
            .keys()
            .map(|t| (t.clone(), standings.verified_session(&cfg.session, t)))
            .collect(),
    );
    let mut entrants = Vec::new();
    for (team, rating) in seeds {
        let jar = teams[&team].build_file().context("équipe sans jar")?;
        let bytes = fs::read(&jar).with_context(|| jar.display().to_string())?;
        fs::write(frozen.join(format!("{team}.jar")), &bytes)?;
        entrants.push(Entrant {
            build: short_hash(&bytes),
            rating,
            multithread: teams[&team].multithread,
            team,
        });
    }
    let draw = Draw {
        v: 1,
        id: id.to_string(),
        ts: now_rfc3339(),
        session: cfg.session.clone(),
        time_ms: settings.time_ms,
        seed,
        entrants,
    };
    let path = draw_path(cfg, id);
    write_atomic(&path, &serde_json::to_vec(&draw)?).with_context(|| path.display().to_string())?;
    Ok(Some(draw))
}

impl Draw {
    pub fn load(cfg: &Config, id: &str) -> anyhow::Result<Self> {
        let path = draw_path(cfg, id);
        let bytes = fs::read(&path).with_context(|| path.display().to_string())?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn teams(&self, cfg: &Config) -> anyhow::Result<Vec<Team>> {
        let frozen = frozen_dir(cfg, &self.id).context("[submit] absent")?;
        self.entrants
            .iter()
            .map(|e| {
                let id: TeamId = e.team.parse()?;
                Ok(cfg.team_with_jar(&id, &frozen.join(format!("{}.jar", e.team)), e.multithread))
            })
            .collect()
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

pub fn draw_path(cfg: &Config, id: &str) -> PathBuf {
    cfg.data_dir().join("tournaments").join(format!("{id}.json"))
}

fn frozen_dir(cfg: &Config, id: &str) -> Option<PathBuf> {
    cfg.submit.as_ref().map(|s| s.jars.join("tournaments").join(id))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    #[test]
    fn ids_get_a_suffix_when_the_day_is_taken() {
        let date = Date::constant(2026, 11, 8);
        assert_eq!(tournament_id(date, |_| false), "tournoi-2026-11-08");
        let taken = ["tournoi-2026-11-08", "tournoi-2026-11-08-2"];
        assert_eq!(tournament_id(date, |id| taken.contains(&id)), "tournoi-2026-11-08-3");
    }

    #[test]
    fn seeding_orders_by_rating_then_arrival_then_name() {
        let seeds = seeding(vec![
            ("new-b".into(), None),
            ("low".into(), Some((1400.0, 0))),
            ("tie-late".into(), Some((1600.0, 5))),
            ("new-a".into(), None),
            ("tie-early".into(), Some((1600.0, 2))),
            ("even".into(), Some((START, 1))),
        ]);
        let names: Vec<&str> = seeds.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(names, ["tie-early", "tie-late", "even", "new-a", "new-b", "low"]);
        assert_eq!(seeds[3].1, START);
    }

    pub(crate) fn setup(teams: &[&str]) -> (PathBuf, Config) {
        let dir = std::env::temp_dir().join(format!("isolation-draw-{}-{}", std::process::id(), teams.len()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\n\
             [submit]\nteams = {teams:?}\njars = {jars:?}\n[tournament]\nmin_teams = 3\n",
            out = dir.join("data/games.jsonl"),
            teams = dir.join("teams.toml"),
            jars = dir.join("jars"),
        );
        let cfg: Config = toml.parse().unwrap();
        let submit = cfg.submit.clone().unwrap();
        for team in teams {
            let id: TeamId = team.parse().unwrap();
            submit::add_team(&submit.teams, id.clone()).unwrap();
            let jar = submit.current_jar(&id);
            fs::create_dir_all(jar.parent().unwrap()).unwrap();
            fs::write(jar, format!("jar de {team}")).unwrap();
        }
        fs::create_dir_all(cfg.data_dir()).unwrap();
        (dir, cfg)
    }

    #[test]
    fn draw_freezes_each_jar_and_saves_the_draw() {
        let (dir, cfg) = setup(&["eq03", "eq01", "eq02"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        let draw = draw(&cfg, &standings, "tournoi-2026-11-08", 42).unwrap().unwrap();
        let names: Vec<&str> = draw.entrants.iter().map(|e| e.team.as_str()).collect();
        assert_eq!(names, ["eq01", "eq02", "eq03"]);
        assert_eq!(draw.entrants[0].build, short_hash(b"jar de eq01"));
        assert_eq!((draw.seed, draw.time_ms, draw.session.as_str()), (42, 5000, "s"));

        let current = cfg.submit.as_ref().unwrap().current_jar(&"eq01".parse().unwrap());
        fs::write(current, "nouvelle version").unwrap();
        let teams = draw.teams(&cfg).unwrap();
        assert_eq!(fs::read(teams[0].build_file().unwrap()).unwrap(), b"jar de eq01");
        assert_eq!(Draw::load(&cfg, "tournoi-2026-11-08").unwrap(), draw);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn too_few_teams_skip_the_draw() {
        let (dir, cfg) = setup(&["eq01", "eq02"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        assert!(draw(&cfg, &standings, "tournoi-2026-11-08", 1).unwrap().is_none());
        assert!(!draw_path(&cfg, "tournoi-2026-11-08").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
