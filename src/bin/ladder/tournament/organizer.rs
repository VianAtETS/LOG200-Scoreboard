use std::collections::HashSet;
use std::fs;
use std::time::Duration;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use isolation::notation;
use isolation::record::{GameRecord, TournamentGame};

use super::admin::{self, Request};
use super::calendar::{Calendar, now};
use super::draw::Entrant;
use super::{Pending, Tournament, discard_jars, draw_path, resume, start, tournament_id, write_atomic};
use crate::config::{Config, TournamentConfig};
use crate::standings::Standings;

#[derive(Deserialize)]
struct Listed {
    id: String,
    ts: String,
    entrants: Vec<Entrant>,
    #[serde(default)]
    winner: Option<String>,
}

#[derive(Serialize)]
struct Entry {
    id: String,
    ts: String,
    teams: usize,
    winner: Option<String>,
}

#[derive(Serialize)]
struct Index {
    v: u32,
    next: Option<String>,
    tournaments: Vec<Entry>,
}

pub struct Organizer {
    enabled: bool,
    settings: TournamentConfig,
    current: Option<Tournament>,
    calendar: Option<Calendar>,
    due: Option<Date>,
}

impl Organizer {
    /// `enabled` faux (`--rounds`) : aucun tournoi.
    pub fn new(cfg: &Config, enabled: bool, games: &[GameRecord]) -> anyhow::Result<Self> {
        let mut organizer = Self {
            enabled,
            settings: cfg.tournament.clone().unwrap_or_default(),
            current: None,
            calendar: None,
            due: None,
        };
        if !enabled {
            return Ok(organizer);
        }
        organizer.current = resume(cfg, games).unwrap_or_else(|e| {
            eprintln!("tournoi en cours non repris : {e:#}");
            None
        });
        if let Some(weekly) = organizer.settings.weekly {
            let now = now()?;
            organizer.calendar = Some(Calendar::new(weekly, &now));
            organizer.due = weekly
                .missed(&now, Duration::from_hours(organizer.settings.catch_up_h))
                .map(|slot| slot.date())
                .filter(|&date| !draw_path(cfg, &tournament_id(date, |_| false)).exists());
        }
        organizer.publish(cfg);
        Ok(organizer)
    }

    pub fn tick(&mut self, cfg: &Config, standings: &Standings, seed: u64) {
        if let Some(calendar) = self.calendar.as_mut()
            && let Ok(now) = now()
            && let Some(date) = calendar.due_at(&now)
        {
            self.due = Some(date);
        }
        if let Some(date) = self.due.take() {
            self.open(cfg, standings, date, seed);
        }
    }

    pub fn poll(&mut self, cfg: &Config, standings: &Standings, seed: u64) {
        if !self.enabled {
            return;
        }
        for request in admin::take(cfg) {
            match request {
                Request::Start => match now() {
                    Ok(now) => self.open(cfg, standings, now.date(), seed),
                    Err(e) => eprintln!("tournoi non tiré : {e}"),
                },
                Request::Resume { id, match_id } => self.resume(cfg, &id, &match_id),
            }
        }
    }

    fn resume(&mut self, cfg: &Config, id: &str, match_id: &str) {
        let Some(t) = self.current.as_mut().filter(|t| t.draw.id == id) else {
            eprintln!("aucun tournoi {id} en cours");
            return;
        };
        if t.resume(match_id) {
            eprintln!("   {id} {match_id} repris");
            self.publish(cfg);
        } else {
            eprintln!("   {id} {match_id} n'est pas suspendu");
        }
    }

    pub fn open(&mut self, cfg: &Config, standings: &Standings, date: Date, seed: u64) {
        if self.current.is_some() {
            eprintln!("tournoi du {date} sauté : le précédent n'est pas fini");
            return;
        }
        match start(cfg, standings, date, seed) {
            Ok(Some(t)) => {
                eprintln!("== tournoi {} : {} équipes", t.draw.id, t.draw.entrants.len());
                self.current = Some(t);
                self.publish(cfg);
            }
            Ok(None) => eprintln!("tournoi du {date} sauté : moins de {} équipes", self.settings.min_teams),
            Err(e) => eprintln!("tournoi du {date} non tiré : {e:#}"),
        }
    }

    pub fn pending(&self, busy: &HashSet<String>) -> Vec<Pending> {
        self.current.as_ref().map(|t| t.pending(busy)).unwrap_or_default()
    }

    pub fn start(&mut self, cfg: &Config, p: &Pending) {
        if let Some(t) = self.current.as_mut() {
            t.start(p.match_index);
            self.publish(cfg);
            eprintln!(
                "== {} {} manche {} : {} contre {} (ouverture {})",
                p.game.id,
                p.game.match_id,
                p.game.game,
                p.red.id,
                p.black.id,
                notation::join(&p.opening)
            );
        }
    }

    pub fn waiting(&self) -> HashSet<String> {
        self.current.as_ref().map(Tournament::waiting).unwrap_or_default()
    }

    /// `played` absent : partie annulée.
    pub fn record(&mut self, cfg: &Config, game: &TournamentGame, played: Option<&GameRecord>) {
        let Some(t) = self.current.as_mut().filter(|t| t.draw.id == game.id) else {
            return;
        };
        if t.finish(&game.match_id, played) {
            eprintln!(
                "   {} {} suspendu après {} annulations",
                game.id,
                game.match_id,
                self.settings.retries + 1
            );
        }
        if let Some(champion) = t.winner() {
            eprintln!("== tournoi {} : {champion} gagne", game.id);
            self.publish(cfg);
            if let Err(e) = discard_jars(cfg, &game.id) {
                eprintln!("   jars figés de {} non supprimés : {e}", game.id);
            }
            self.current = None;
        } else {
            self.publish(cfg);
        }
    }

    fn publish(&self, cfg: &Config) {
        if let Some(t) = &self.current {
            let path = draw_path(cfg, &t.draw.id);
            let written = serde_json::to_vec(&t.view())
                .map_err(std::io::Error::from)
                .and_then(|bytes| write_atomic(&path, &bytes));
            if let Err(e) = written {
                eprintln!("   tableau {} non publié : {e}", t.draw.id);
            }
        }
        if let Err(e) = self.write_index(cfg) {
            eprintln!("   index des tournois non publié : {e}");
        }
    }

    fn write_index(&self, cfg: &Config) -> std::io::Result<()> {
        let dir = cfg.data_dir().join("tournaments");
        let mut listed: Vec<Listed> = fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
                    .filter_map(|e| serde_json::from_slice(&fs::read(e.path()).ok()?).ok())
                    .collect()
            })
            .unwrap_or_default();
        listed.sort_by(|a, b| b.ts.cmp(&a.ts));
        let index = Index {
            v: 1,
            next: self
                .calendar
                .as_ref()
                .and_then(Calendar::next)
                .map(|z| z.timestamp().to_string()),
            tournaments: listed
                .into_iter()
                .map(|l| Entry {
                    id: l.id,
                    ts: l.ts,
                    teams: l.entrants.len(),
                    winner: l.winner,
                })
                .collect(),
        };
        write_atomic(&dir.join("index.json"), &serde_json::to_vec(&index)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;

    use super::super::admin::{self, Request};
    use super::super::draw;
    use super::*;

    fn read(path: &std::path::Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn admin_requests_start_a_tournament_and_resume_a_suspended_match() {
        let (dir, cfg) = draw::tests::setup(&["eq01", "eq02", "eq03", "eq04", "eq05", "eq06"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        let mut organizer = Organizer::new(&cfg, true, &[]).unwrap();
        admin::send(&cfg, &Request::Start).unwrap();
        organizer.poll(&cfg, &standings, 1);
        let p = organizer.pending(&HashSet::new()).remove(0);
        for _ in 0..4 {
            organizer.start(&cfg, &p);
            organizer.record(&cfg, &p.game, None);
        }
        let blocked = |o: &Organizer| {
            o.pending(&HashSet::new())
                .iter()
                .all(|q| q.match_index != p.match_index)
        };
        assert!(blocked(&organizer));
        let resume = Request::Resume {
            id: p.game.id.clone(),
            match_id: p.game.match_id.clone(),
        };
        admin::send(&cfg, &resume).unwrap();
        organizer.poll(&cfg, &standings, 1);
        assert!(!blocked(&organizer));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_opened_tournament_runs_to_its_winner_then_closes() {
        let (dir, cfg) = draw::tests::setup(&["eq01", "eq02", "eq03", "eq04", "eq05"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        let mut organizer = Organizer::new(&cfg, true, &[]).unwrap();
        assert!(organizer.pending(&HashSet::new()).is_empty());
        organizer.open(&cfg, &standings, Date::constant(2026, 11, 8), 3);
        let first = organizer.pending(&HashSet::new());
        assert_eq!(first[0].time_ms, 5000);
        let index = cfg.data_dir().join("tournaments/index.json");
        let id = "tournoi-2026-11-08";
        assert_eq!(read(&index)["tournaments"][0]["id"], id);
        assert!(read(&index)["tournaments"][0]["winner"].is_null());
        assert!(read(&index)["next"].is_null());
        let view = read(&draw_path(&cfg, id));
        assert_eq!(view["entrants"].as_array().unwrap().len(), 5);
        assert_eq!(view["matches"][0]["status"], "done");
        organizer.open(&cfg, &standings, Date::constant(2026, 11, 8), 4);
        assert_eq!(organizer.pending(&HashSet::new()).len(), first.len());
        let mut history = Vec::new();
        while let Some(p) = organizer.pending(&HashSet::new()).into_iter().next() {
            organizer.start(&cfg, &p);
            assert_eq!(
                read(&draw_path(&cfg, id))["matches"][p.match_index]["status"],
                "running"
            );
            assert!(
                organizer
                    .pending(&HashSet::new())
                    .iter()
                    .all(|q| q.match_index != p.match_index)
            );
            history.push(super::super::tests::record(&p, true));
            organizer.record(&cfg, &p.game, history.last());
            assert!(history.len() < 100);
        }
        assert!(organizer.waiting().is_empty());
        let view = read(&draw_path(&cfg, id));
        assert!(view["winner"].is_string());
        assert_eq!(view["places"].as_array().unwrap().len(), 5);
        assert_eq!(read(&index)["tournaments"][0]["winner"], view["winner"]);
        assert!(!cfg.submit.as_ref().unwrap().jars.join("tournaments").join(id).exists());
        assert!(
            Organizer::new(&cfg, true, &history)
                .unwrap()
                .pending(&HashSet::new())
                .is_empty()
        );
        assert!(!draw_path(&cfg, "tournoi-2026-11-08-2").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
