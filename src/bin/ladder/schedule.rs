//! Choix des paires : la moins jouée de la session, pour que le ladder reprenne là où il s'est
//! arrêté, parmi les équipes de niveau voisin.

use std::collections::{HashMap, HashSet};

use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

use isolation::record::GameRecord;

use crate::config::Team;

const MAX_GAP: f64 = 600.0;
const CATCH_UP: u32 = 4;
const SLACK: u32 = CATCH_UP;

/// Paire d'équipes, sans ordre.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct PairKey(String, String);

impl PairKey {
    fn new(a: &str, b: &str) -> Self {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        Self(first.to_string(), second.to_string())
    }
}

#[derive(Default)]
pub struct Scheduler {
    played: HashMap<PairKey, u32>,
    base: HashMap<String, u32>,
    bots: HashSet<String>,
}

impl Scheduler {
    pub fn from_history(games: &[GameRecord], session: &str) -> Self {
        let mut scheduler = Self::default();
        for game in games.iter().filter(|g| g.session == session) {
            scheduler.join(&game.red.team, game.red.build.is_none());
            scheduler.join(&game.black.team, game.black.build.is_none());
            scheduler.record(&game.red.team, &game.black.team, 1);
        }
        scheduler
    }

    pub fn record(&mut self, a: &str, b: &str, games: u32) {
        *self.played.entry(PairKey::new(a, b)).or_default() += games;
    }

    fn played(&self, a: &str, b: &str) -> u32 {
        self.played.get(&PairKey::new(a, b)).copied().unwrap_or(0)
    }

    fn weight(&self, a: &str, b: &str) -> u32 {
        1 << (u32::from(self.bots.contains(a)) + u32::from(self.bots.contains(b)))
    }

    fn effective(&self, a: &str, b: &str) -> u32 {
        let base = |id: &str| self.base.get(id).copied().unwrap_or(0);
        self.played(a, b) * self.weight(a, b) + base(a).max(base(b))
    }

    fn join(&mut self, id: &str, bot: bool) {
        if self.base.contains_key(id) {
            return;
        }
        let level = self
            .played
            .keys()
            .filter(|PairKey(a, b)| self.base.contains_key(a) && self.base.contains_key(b))
            .map(|PairKey(a, b)| (self.effective(a, b) + 2).saturating_sub(2 * self.weight(a, b)))
            .max()
            .unwrap_or(0);
        self.base.insert(id.to_string(), level.saturating_sub(CATCH_UP));
        if bot {
            self.bots.insert(id.to_string());
        }
    }

    pub fn next_pair<'t>(
        &mut self,
        teams: &'t [Team],
        busy: &HashSet<String>,
        rating: impl Fn(&str) -> f64,
        rng: &mut impl Rng,
    ) -> Option<(&'t Team, &'t Team)> {
        for team in teams {
            self.join(&team.id, team.build_file().is_none());
        }
        let ratings: Vec<f64> = teams.iter().map(|t| rating(&t.id)).collect();
        let gap = |i: usize, j: usize| (ratings[i] - ratings[j]).abs();
        let nearest: Vec<Option<usize>> = (0..teams.len())
            .map(|i| {
                (0..teams.len())
                    .filter(|&j| j != i)
                    .min_by(|&x, &y| gap(i, x).total_cmp(&gap(i, y)))
            })
            .collect();
        let pairs: Vec<(&Team, &Team, u32)> = (0..teams.len())
            .flat_map(|i| (i + 1..teams.len()).map(move |j| (i, j)))
            .filter(|&(i, j)| gap(i, j) <= MAX_GAP || nearest[i] == Some(j) || nearest[j] == Some(i))
            .map(|(i, j)| (&teams[i], &teams[j], self.effective(&teams[i].id, &teams[j].id)))
            .collect();
        let fewest = pairs.iter().map(|&(.., n)| n).min()?;
        let free: Vec<_> = pairs
            .into_iter()
            .filter(|&(a, b, n)| n <= fewest + SLACK && !busy.contains(&a.id) && !busy.contains(&b.id))
            .collect();
        let lowest = free.iter().map(|&(.., n)| n).min()?;
        let least_played: Vec<_> = free.into_iter().filter(|&(.., n)| n == lowest).collect();
        let &(a, b, _) = least_played.choose(rng)?;
        Some(if rng.random() { (a, b) } else { (b, a) })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::config::CommandTemplate;

    fn team(id: &str) -> Team {
        Team {
            id: id.into(),
            cmd: CommandTemplate::default(),
            build: Some(format!("{id}.jar").into()),
            anchor: false,
        }
    }

    fn bot(id: &str) -> Team {
        Team {
            build: None,
            ..team(id)
        }
    }

    fn play_rounds(scheduler: &mut Scheduler, teams: &[Team], rounds: usize, rng: &mut StdRng) -> Vec<[String; 2]> {
        (0..rounds)
            .map(|_| {
                let (x, y) = scheduler.next_pair(teams, &HashSet::new(), |_| 1500.0, rng).unwrap();
                let pair = [x.id.clone(), y.id.clone()];
                scheduler.record(&pair[0], &pair[1], 2);
                pair
            })
            .collect()
    }

    #[test]
    fn pvp_plays_twice_as_often_as_pve_and_four_times_eve() {
        let teams = [team("s1"), team("s2"), bot("e1"), bot("e2")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        play_rounds(&mut scheduler, &teams, 5 * 13, &mut rng);
        assert_eq!(scheduler.played("s1", "s2"), 40);
        for e in ["e1", "e2"] {
            assert_eq!(scheduler.played("s1", e), 20);
            assert_eq!(scheduler.played("s2", e), 20);
        }
        assert_eq!(scheduler.played("e1", "e2"), 10);
    }

    #[test]
    fn late_student_after_bots_only_catches_up_briefly() {
        let mut teams = vec![bot("e1"), bot("e2"), bot("e3")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        play_rounds(&mut scheduler, &teams, 30, &mut rng);
        teams.push(team("s"));
        let pairs = play_rounds(&mut scheduler, &teams, 30, &mut rng);
        let first_eve = pairs.iter().position(|p| !p.contains(&"s".to_string())).unwrap();
        assert!((6..=9).contains(&first_eve), "{first_eve}");
    }

    #[test]
    fn picks_the_least_played_pair() {
        let teams = [team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 4);
        scheduler.record("c", "a", 2);
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..20 {
            let (x, y) = scheduler
                .next_pair(&teams, &HashSet::new(), |_| 1500.0, &mut rng)
                .unwrap();
            let mut ids = [x.id.as_str(), y.id.as_str()];
            ids.sort_unstable();
            assert_eq!(ids, ["b", "c"]);
        }
    }

    #[test]
    fn late_team_catches_up_only_briefly() {
        let mut teams = vec![team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        scheduler.next_pair(&teams, &HashSet::new(), |_| 1500.0, &mut rng);
        for (x, y) in [("a", "b"), ("a", "c"), ("b", "c")] {
            scheduler.record(x, y, 20);
        }
        teams.push(team("d"));
        let mut play = || {
            let (x, y) = scheduler
                .next_pair(&teams, &HashSet::new(), |_| 1500.0, &mut rng)
                .unwrap();
            let pair = [x.id.clone(), y.id.clone()];
            scheduler.record(&pair[0], &pair[1], 2);
            pair.contains(&"d".to_string())
        };
        let head_start = 3 * CATCH_UP as usize / 2;
        assert!((0..head_start).all(|_| play()));
        let next: Vec<bool> = (0..6).map(|_| play()).collect();
        assert_eq!(next.iter().filter(|&&with_d| with_d).count(), 3);
    }

    #[test]
    fn teams_present_from_start_get_no_head_start() {
        let teams = [team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..30 {
            let (x, y) = scheduler
                .next_pair(&teams, &HashSet::new(), |_| 1500.0, &mut rng)
                .unwrap();
            let (x, y) = (x.id.clone(), y.id.clone());
            scheduler.record(&x, &y, 2);
        }
        for (x, y) in [("a", "b"), ("a", "c"), ("b", "c")] {
            assert_eq!(scheduler.effective(x, y), 20);
        }
    }

    #[test]
    fn needs_two_teams() {
        let mut rng = StdRng::seed_from_u64(3);
        assert!(
            Scheduler::default()
                .next_pair(&[team("a")], &HashSet::new(), |_| 1500.0, &mut rng)
                .is_none()
        );
    }

    /// Paires tirées sur 50 essais, triées.
    fn drawn(scheduler: &mut Scheduler, teams: &[Team], rating: impl Fn(&str) -> f64) -> BTreeSet<[String; 2]> {
        let mut rng = StdRng::seed_from_u64(3);
        (0..50)
            .map(|_| {
                let (x, y) = scheduler.next_pair(teams, &HashSet::new(), &rating, &mut rng).unwrap();
                let mut ids = [x.id.clone(), y.id.clone()];
                ids.sort_unstable();
                ids
            })
            .collect()
    }

    fn pair(a: &str, b: &str) -> [String; 2] {
        [a.into(), b.into()]
    }

    #[test]
    fn skips_pairs_too_far_apart() {
        let teams = [team("a"), team("b"), team("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1600.0,
            _ => 2500.0,
        };
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 4);
        scheduler.record("b", "c", 4);
        assert_eq!(
            drawn(&mut scheduler, &teams, rating),
            [pair("a", "b"), pair("b", "c")].into()
        );
    }

    #[test]
    fn isolated_team_still_plays_its_nearest() {
        let teams = [team("a"), team("b"), team("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1550.0,
            _ => 3000.0,
        };
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 4);
        assert_eq!(drawn(&mut scheduler, &teams, rating), [pair("b", "c")].into());
    }

    #[test]
    fn busy_teams_are_left_out() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 2);
        let busy = HashSet::from(["a".to_string(), "b".to_string()]);
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..20 {
            let (x, y) = scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng).unwrap();
            let mut ids = [x.id.as_str(), y.id.as_str()];
            ids.sort_unstable();
            assert_eq!(ids, ["c", "d"]);
        }
    }

    #[test]
    fn free_teams_replay_only_within_slack() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let busy = HashSet::from(["c".to_string(), "d".to_string()]);
        let mut rng = StdRng::seed_from_u64(3);
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", SLACK);
        assert!(scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng).is_some());
        scheduler.record("a", "b", 2);
        // a-c, a-d, b-c et b-d sont restées à 0 : a et b attendent c et d.
        assert!(scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng).is_none());
    }

    #[test]
    fn slots_stay_busy_while_a_late_team_catches_up() {
        let mut teams = vec![team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        scheduler.next_pair(&teams, &HashSet::new(), |_| 1500.0, &mut rng);
        for (x, y) in [("a", "b"), ("a", "c"), ("b", "c")] {
            scheduler.record(x, y, 20);
        }
        teams.push(team("d"));
        let busy = HashSet::from(["d".to_string()]);
        let (x, y) = scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng).unwrap();
        assert!(x.id != "d" && y.id != "d");
    }

    /// Deux slots, quatre équipes : sans borne, a-b et c-d se rejoueraient en boucle.
    #[test]
    fn no_pair_starves_with_two_slots() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        let mut running = std::collections::VecDeque::new();
        for _ in 0..60 {
            let busy: HashSet<String> = running.iter().flat_map(|p: &[String; 2]| p.clone()).collect();
            match scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng) {
                Some((x, y)) if running.len() < 2 => {
                    let pair = [x.id.clone(), y.id.clone()];
                    scheduler.record(&pair[0], &pair[1], 2);
                    running.push_back(pair);
                }
                _ => {
                    running.pop_front();
                }
            }
        }
        let counts: Vec<u32> = ["a", "b", "c", "d"]
            .iter()
            .enumerate()
            .flat_map(|(i, x)| ["a", "b", "c", "d"][i + 1..].iter().map(|y| scheduler.played(x, y)))
            .collect();
        let spread = counts.iter().max().unwrap() - counts.iter().min().unwrap();
        assert!(spread <= SLACK + 2, "{counts:?}");
    }
}
