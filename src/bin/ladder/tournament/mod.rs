mod bracket;
pub mod calendar;
mod draw;
mod organizer;

use std::collections::HashSet;
use std::fs;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use jiff::civil::Date;

use isolation::board::{Color, Move};
use isolation::record::{GameRecord, TournamentGame};
use isolation::rules::random_opening;

use crate::config::{Config, Team};
use crate::standings::Standings;
use bracket::{Bracket, Status};
pub use draw::{Draw, draw, draw_path, tournament_id};
pub use organizer::Organizer;

/// `None` : trop peu d'équipes.
pub fn start(cfg: &Config, standings: &Standings, date: Date, seed: u64) -> anyhow::Result<Option<Tournament>> {
    let id = tournament_id(date, |id| draw_path(cfg, id).exists());
    let Some(draw) = draw(cfg, standings, &id, seed)? else {
        return Ok(None);
    };
    let teams = draw.teams(cfg)?;
    Ok(Some(Tournament::new(draw, teams, cfg.opening_plies, retries(cfg))))
}

/// Le dernier tournoi tiré, s'il n'est pas fini.
pub fn resume(cfg: &Config, games: &[GameRecord]) -> anyhow::Result<Option<Tournament>> {
    let Ok(entries) = fs::read_dir(cfg.data_dir().join("tournaments")) else {
        return Ok(None);
    };
    let latest = entries
        .flatten()
        .filter_map(|e| Draw::load(cfg, e.file_name().to_str()?.strip_suffix(".json")?).ok())
        .max_by(|a, b| a.ts.cmp(&b.ts));
    let Some(draw) = latest else {
        return Ok(None);
    };
    let teams = draw.teams(cfg)?;
    let mut tournament = Tournament::new(draw, teams, cfg.opening_plies, retries(cfg));
    tournament.replay(games);
    Ok(tournament.winner().is_none().then_some(tournament))
}

fn retries(cfg: &Config) -> u32 {
    cfg.tournament.clone().unwrap_or_default().retries
}

pub struct Pending {
    pub match_index: usize,
    pub game: TournamentGame,
    pub red: Team,
    pub black: Team,
    pub opening: Vec<Move>,
    pub time_ms: u64,
}

pub struct Tournament {
    pub draw: Draw,
    bracket: Bracket,
    teams: Vec<Team>,
    opening_plies: usize,
    retries: u32,
    wins: Vec<[u32; 2]>,
    failures: Vec<u32>,
    running: Vec<bool>,
    suspended: Vec<bool>,
}

impl Tournament {
    pub fn new(draw: Draw, teams: Vec<Team>, opening_plies: usize, retries: u32) -> Self {
        let bracket = Bracket::new(teams.len());
        let n = bracket.matches.len();
        Self {
            draw,
            bracket,
            teams,
            opening_plies,
            retries,
            wins: vec![[0; 2]; n],
            failures: vec![0; n],
            running: vec![false; n],
            suspended: vec![false; n],
        }
    }

    pub fn replay(&mut self, games: &[GameRecord]) {
        for game in games {
            let Some(t) = game.tournament.as_ref().filter(|t| t.id == self.draw.id) else {
                continue;
            };
            let winner = match game.result.winner {
                Color::Red => &game.red.team,
                Color::Black => &game.black.team,
            };
            if let Some(k) = self.index(&t.match_id) {
                self.win(k, winner);
            }
        }
    }

    pub fn pending(&self, busy: &HashSet<String>) -> Vec<Pending> {
        self.ready()
            .into_iter()
            .filter(|(_, seeds)| seeds.iter().all(|&s| !busy.contains(&self.teams[s].id)))
            .map(|(k, seeds)| self.game(k, seeds))
            .collect()
    }

    /// Équipes d'un match prêt : le ladder les laisse libres.
    pub fn waiting(&self) -> HashSet<String> {
        self.ready()
            .into_iter()
            .flat_map(|(_, seeds)| seeds.map(|s| self.teams[s].id.clone()))
            .collect()
    }

    pub fn start(&mut self, match_index: usize) {
        self.running[match_index] = true;
    }

    /// `winner` absent : partie annulée. Vrai si le match vient d'être suspendu.
    pub fn finish(&mut self, match_id: &str, winner: Option<&str>) -> bool {
        let Some(k) = self.index(match_id) else {
            return false;
        };
        self.running[k] = false;
        if let Some(team) = winner {
            self.failures[k] = 0;
            self.win(k, team);
            return false;
        }
        self.failures[k] += 1;
        let suspend = self.failures[k] > self.retries && !self.suspended[k];
        self.suspended[k] |= suspend;
        suspend
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "commande admin, étape 8"))]
    pub fn resume(&mut self, match_id: &str) -> bool {
        let Some(k) = self.index(match_id).filter(|&k| self.suspended[k]) else {
            return false;
        };
        self.suspended[k] = false;
        self.failures[k] = 0;
        true
    }

    pub fn winner(&self) -> Option<&str> {
        match self.bracket.statuses(&self.wins).last()? {
            Status::Done { winner: Some(s), .. } => Some(&self.teams[*s].id),
            _ => None,
        }
    }

    fn index(&self, match_id: &str) -> Option<usize> {
        self.bracket.matches.iter().position(|m| m.id == match_id)
    }

    fn ready(&self) -> Vec<(usize, [usize; 2])> {
        self.bracket
            .statuses(&self.wins)
            .into_iter()
            .enumerate()
            .filter_map(|(k, s)| match s {
                Status::Ready(seeds) if !self.running[k] && !self.suspended[k] => Some((k, seeds)),
                _ => None,
            })
            .collect()
    }

    fn win(&mut self, k: usize, team: &str) {
        if let Status::Ready(seeds) = self.bracket.statuses(&self.wins)[k]
            && let Some(slot) = seeds.iter().position(|&s| self.teams[s].id == team)
        {
            self.wins[k][slot] += 1;
        }
    }

    /// Une ouverture par paire de manches ; la belle d'un meilleur de 3 tire ses couleurs au sort.
    fn game(&self, k: usize, seeds: [usize; 2]) -> Pending {
        let m = &self.bracket.matches[k];
        let played = self.wins[k][0] + self.wins[k][1];
        let mut rng = StdRng::seed_from_u64(self.draw.seed ^ (k as u64) << 16 ^ u64::from(played / 2));
        let opening = random_opening(self.opening_plies, &mut rng);
        let first_red = if played + 1 == m.best_of {
            rng.random()
        } else {
            played.is_multiple_of(2)
        };
        let [red, black] = if first_red { seeds } else { [seeds[1], seeds[0]] };
        Pending {
            match_index: k,
            game: TournamentGame {
                id: self.draw.id.clone(),
                match_id: m.id.clone(),
                game: played + 1,
            },
            red: self.teams[red].clone(),
            black: self.teams[black].clone(),
            opening,
            time_ms: self.draw.time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use isolation::record::{GameResult, InvalidCounts, Player, RECORD_VERSION, Reason};

    use super::*;
    use crate::config::CommandTemplate;

    use draw::Entrant;

    fn tournament(n: usize) -> Tournament {
        let names: Vec<String> = (0..n).map(|i| format!("t{i}")).collect();
        let draw = Draw {
            v: 1,
            id: "tournoi-2026-11-08".into(),
            ts: "2026-11-08T17:00:00.000Z".into(),
            session: "s".into(),
            time_ms: 5000,
            seed: 7,
            entrants: names
                .iter()
                .map(|team| Entrant {
                    team: team.clone(),
                    build: "b".into(),
                    rating: 1500.0,
                    multithread: false,
                })
                .collect(),
        };
        let teams = names
            .iter()
            .map(|id| Team {
                id: id.clone(),
                cmd: CommandTemplate::default(),
                build: None,
                anchor: false,
                multithread: false,
                remote: None,
            })
            .collect();
        Tournament::new(draw, teams, 2, 3)
    }

    fn summary(pending: &[Pending]) -> Vec<(String, u32, String, String)> {
        pending
            .iter()
            .map(|p| {
                (
                    p.game.match_id.clone(),
                    p.game.game,
                    p.red.id.clone(),
                    p.black.id.clone(),
                )
            })
            .collect()
    }

    fn opening_of(t: &Tournament, match_id: &str) -> Vec<Move> {
        let pending = t.pending(&HashSet::new());
        pending
            .into_iter()
            .find(|p| p.game.match_id == match_id)
            .unwrap()
            .opening
    }

    fn record(p: &Pending, red_wins: bool) -> GameRecord {
        let player = |team: &Team| Player {
            team: team.id.clone(),
            build: None,
            anchor: false,
            remote: false,
        };
        GameRecord {
            id: format!("{}-{}", p.game.match_id, p.game.game),
            session: p.game.id.clone(),
            ts: "2026-11-08T17:00:00.000Z".into(),
            red: player(&p.red),
            black: player(&p.black),
            tournament: Some(p.game.clone()),
            result: GameResult {
                v: RECORD_VERSION,
                time_ms: 5000,
                opening: vec![],
                moves: vec![],
                winner: if red_wins { Color::Red } else { Color::Black },
                reason: Reason::NoMoves,
                invalid: InvalidCounts::default(),
                duration_ms: 0,
                move_ms: vec![],
            },
        }
    }

    #[test]
    fn first_games_follow_the_seeding_and_swap_colours_on_the_same_opening() {
        let mut t = tournament(4);
        let first = t.pending(&HashSet::new());
        assert_eq!(
            summary(&first),
            [
                ("W1".into(), 1, "t0".into(), "t3".into()),
                ("W2".into(), 1, "t1".into(), "t2".into())
            ]
        );
        assert_eq!(first[0].opening.len(), 2);
        t.start(first[0].match_index);
        assert!(!t.finish("W1", Some("t0")));
        let second = t.pending(&HashSet::new());
        assert_eq!(summary(&second)[0], ("W1".into(), 2, "t3".into(), "t0".into()));
        assert_eq!(second[0].opening, first[0].opening);
    }

    #[test]
    fn the_deciding_game_gets_a_new_opening() {
        let mut t = tournament(4);
        let opening = opening_of(&t, "W1");
        t.finish("W1", Some("t0"));
        t.finish("W1", Some("t0"));
        assert!(t.pending(&HashSet::new()).iter().all(|p| p.game.match_id != "W1"));
        let mut u = tournament(4);
        u.finish("W1", Some("t0"));
        u.finish("W1", Some("t3"));
        assert_eq!(summary(&u.pending(&HashSet::new()))[0].1, 3);
        assert_ne!(opening_of(&u, "W1"), opening);
    }

    #[test]
    fn busy_or_running_matches_wait() {
        let mut t = tournament(4);
        let busy = HashSet::from(["t0".to_string()]);
        assert_eq!(summary(&t.pending(&busy)), [("W2".into(), 1, "t1".into(), "t2".into())]);
        t.start(1);
        assert!(summary(&t.pending(&HashSet::new())).iter().all(|p| p.0 == "W1"));
        assert_eq!(t.waiting(), HashSet::from(["t0".to_string(), "t3".to_string()]));
    }

    #[test]
    fn repeated_cancellations_suspend_the_match_until_resumed() {
        let mut t = tournament(4);
        for _ in 0..3 {
            assert!(!t.finish("W1", None));
        }
        assert!(t.finish("W1", None));
        assert!(t.pending(&HashSet::new()).iter().all(|p| p.game.match_id != "W1"));
        assert!(!t.waiting().contains("t0"));
        assert!(t.resume("W1"));
        assert!(!t.resume("W1"));
        assert_eq!(summary(&t.pending(&HashSet::new()))[0].0, "W1");
    }

    #[test]
    fn replay_rebuilds_progress_from_records() {
        let mut live = tournament(4);
        let mut history = Vec::new();
        for _ in 0..2 {
            let p = live.pending(&HashSet::new()).remove(0);
            history.push(record(&p, p.red.id == "t0"));
            live.finish(&p.game.match_id, Some("t0"));
        }
        let mut other = record(&live.pending(&HashSet::new())[0], true);
        other.tournament.as_mut().unwrap().id = "tournoi-autre".into();
        history.push(other);
        let mut resumed = tournament(4);
        resumed.replay(&history);
        assert_eq!(
            summary(&resumed.pending(&HashSet::new())),
            summary(&live.pending(&HashSet::new()))
        );
    }

    #[test]
    fn a_started_tournament_resumes_from_its_games() {
        let (dir, cfg) = draw::tests::setup(&["eq01", "eq02", "eq03", "eq04"]);
        let standings = Standings::from_history(cfg.data_dir(), &[], &[]).unwrap();
        let date = jiff::civil::Date::constant(2026, 11, 8);
        let mut live = start(&cfg, &standings, date, 9).unwrap().unwrap();
        assert_eq!(live.draw.id, "tournoi-2026-11-08");
        let p = live.pending(&HashSet::new()).remove(0);
        let game = record(&p, true);
        live.finish(&p.game.match_id, Some(&p.red.id));
        let resumed = resume(&cfg, &[game]).unwrap().unwrap();
        assert_eq!(
            summary(&resumed.pending(&HashSet::new())),
            summary(&live.pending(&HashSet::new()))
        );
        let again = start(&cfg, &standings, date, 9).unwrap().unwrap();
        assert_eq!(again.draw.id, "tournoi-2026-11-08-2");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_full_tournament_crowns_a_winner() {
        for n in [4, 5, 9] {
            let mut t = tournament(n);
            let mut games = 0;
            while t.winner().is_none() {
                let pending = t.pending(&HashSet::new());
                assert!(!pending.is_empty(), "{n} équipes : bloqué");
                for p in pending {
                    t.start(p.match_index);
                    let favourite = p.red.id.min(p.black.id);
                    t.finish(&p.game.match_id, Some(&favourite));
                    games += 1;
                }
            }
            assert_eq!(t.winner(), Some("t0"));
            assert!(games > n, "{n} équipes, {games} parties");
        }
    }
}
