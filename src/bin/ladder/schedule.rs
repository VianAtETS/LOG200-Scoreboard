//! Choix des paires : la moins jouée de la session, pour que le ladder reprenne là où il s'est
//! arrêté, parmi les équipes de niveau voisin.

use std::collections::{HashMap, HashSet};

use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

use isolation::record::GameRecord;

use crate::config::Team;

const MAX_GAP: f64 = 600.0;

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
}

impl Scheduler {
    pub fn from_history(games: &[GameRecord], session: &str) -> Self {
        let mut scheduler = Self::default();
        for game in games.iter().filter(|g| g.session == session) {
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

    pub fn next_pair<'t>(
        &self,
        teams: &'t [Team],
        busy: &HashSet<String>,
        rating: impl Fn(&str) -> f64,
        rng: &mut impl Rng,
    ) -> Option<(&'t Team, &'t Team)> {
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
            .map(|(i, j)| (&teams[i], &teams[j], self.played(&teams[i].id, &teams[j].id)))
            .collect();
        let fewest = pairs.iter().map(|&(.., n)| n).min()?;
        let least_played: Vec<_> = pairs
            .into_iter()
            .filter(|&(a, b, n)| n == fewest && !busy.contains(&a.id) && !busy.contains(&b.id))
            .collect();
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
            build: None,
            anchor: false,
        }
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
    fn needs_two_teams() {
        let mut rng = StdRng::seed_from_u64(3);
        assert!(
            Scheduler::default()
                .next_pair(&[team("a")], &HashSet::new(), |_| 1500.0, &mut rng)
                .is_none()
        );
    }

    /// Paires tirées sur 50 essais, triées.
    fn drawn(scheduler: &Scheduler, teams: &[Team], rating: impl Fn(&str) -> f64) -> BTreeSet<[String; 2]> {
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
            drawn(&scheduler, &teams, rating),
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
        assert_eq!(drawn(&scheduler, &teams, rating), [pair("b", "c")].into());
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
    fn free_teams_wait_rather_than_replay() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 2);
        scheduler.record("c", "d", 2);
        // a-b vient de finir, c-d joue encore : a-c, a-d, b-c et b-d attendent c et d.
        let busy = HashSet::from(["c".to_string(), "d".to_string()]);
        let mut rng = StdRng::seed_from_u64(3);
        assert!(scheduler.next_pair(&teams, &busy, |_| 1500.0, &mut rng).is_none());
    }
}
