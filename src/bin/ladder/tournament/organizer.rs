use std::collections::HashSet;
use std::time::Duration;

use jiff::civil::Date;

use isolation::notation;
use isolation::record::{GameRecord, TournamentGame};

use super::calendar::{Calendar, now};
use super::{Pending, Tournament, draw_path, resume, start, tournament_id};
use crate::config::{Config, TournamentConfig};
use crate::standings::Standings;

pub struct Organizer {
    settings: TournamentConfig,
    current: Option<Tournament>,
    calendar: Option<Calendar>,
    due: Option<Date>,
}

impl Organizer {
    /// `enabled` faux (`--rounds`) : aucun tournoi.
    pub fn new(cfg: &Config, enabled: bool, games: &[GameRecord]) -> anyhow::Result<Self> {
        let mut organizer = Self {
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

    pub fn open(&mut self, cfg: &Config, standings: &Standings, date: Date, seed: u64) {
        if self.current.is_some() {
            eprintln!("tournoi du {date} sauté : le précédent n'est pas fini");
            return;
        }
        match start(cfg, standings, date, seed) {
            Ok(Some(t)) => {
                eprintln!("== tournoi {} : {} équipes", t.draw.id, t.draw.entrants.len());
                self.current = Some(t);
            }
            Ok(None) => eprintln!("tournoi du {date} sauté : moins de {} équipes", self.settings.min_teams),
            Err(e) => eprintln!("tournoi du {date} non tiré : {e:#}"),
        }
    }

    pub fn pending(&self, busy: &HashSet<String>) -> Vec<Pending> {
        self.current.as_ref().map(|t| t.pending(busy)).unwrap_or_default()
    }

    pub fn start(&mut self, p: &Pending) {
        if let Some(t) = self.current.as_mut() {
            t.start(p.match_index);
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

    /// `winner` absent : partie annulée.
    pub fn record(&mut self, game: &TournamentGame, winner: Option<&str>) {
        let Some(t) = self.current.as_mut().filter(|t| t.draw.id == game.id) else {
            return;
        };
        if t.finish(&game.match_id, winner) {
            eprintln!(
                "   {} {} suspendu après {} annulations",
                game.id,
                game.match_id,
                self.settings.retries + 1
            );
        }
        if let Some(champion) = t.winner() {
            eprintln!("== tournoi {} : {champion} gagne", game.id);
            self.current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::draw;
    use super::*;

    #[test]
    fn an_opened_tournament_runs_to_its_winner_then_closes() {
        let (dir, cfg) = draw::tests::setup(&["eq01", "eq02", "eq03", "eq04", "eq05"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        let mut organizer = Organizer::new(&cfg, true, &[]).unwrap();
        assert!(organizer.pending(&HashSet::new()).is_empty());
        organizer.open(&cfg, &standings, Date::constant(2026, 11, 8), 3);
        let first = organizer.pending(&HashSet::new());
        assert_eq!(first[0].time_ms, 5000);
        organizer.open(&cfg, &standings, Date::constant(2026, 11, 8), 4);
        assert_eq!(organizer.pending(&HashSet::new()).len(), first.len());
        let mut games = 0;
        while let Some(p) = organizer.pending(&HashSet::new()).into_iter().next() {
            organizer.start(&p);
            assert!(
                organizer
                    .pending(&HashSet::new())
                    .iter()
                    .all(|q| q.match_index != p.match_index)
            );
            organizer.record(&p.game, Some(&p.red.id));
            games += 1;
            assert!(games < 100);
        }
        assert!(organizer.waiting().is_empty());
        assert!(!draw_path(&cfg, "tournoi-2026-11-08-2").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
