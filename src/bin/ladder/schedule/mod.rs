//! Choix des paires : chaque équipe reçoit la même part des parties, contre un adversaire de
//! niveau voisin, de préférence peu affronté. L'état se reconstruit depuis l'historique de la session.

mod queue;

use std::collections::{HashMap, HashSet};

use rand::seq::{IndexedRandom, SliceRandom};
use rand::{Rng, RngExt};

use isolation::record::GameRecord;

use crate::config::Team;
use queue::FairQueue;

const MAX_GAP: f64 = 600.0;
const CATCH_UP: u32 = 20;
const BOT_STRIDE: u32 = 2;

/// Paire d'équipes, sans ordre.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct PairKey(String, String);

impl PairKey {
    fn new(a: &str, b: &str) -> Self {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        Self(first.to_string(), second.to_string())
    }
}

pub struct Scheduler {
    queue: FairQueue,
    games: HashMap<PairKey, u32>,
    last: HashMap<String, String>,
    bots: HashSet<String>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            queue: FairQueue::new(CATCH_UP),
            games: HashMap::new(),
            last: HashMap::new(),
            bots: HashSet::new(),
        }
    }
}

impl Scheduler {
    pub fn from_history(games: &[GameRecord], session: &str) -> Self {
        let mut scheduler = Self::default();
        for game in games.iter().filter(|g| g.session == session) {
            for player in [&game.red, &game.black] {
                scheduler.mark_bot(&player.team, player.build.is_none());
            }
            scheduler.record(&game.red.team, &game.black.team, 1);
        }
        scheduler.queue.forget_leads();
        scheduler
    }

    pub fn record(&mut self, a: &str, b: &str, games: u32) {
        self.queue.advance(self.queue.pass(a).min(self.queue.pass(b)));
        self.queue.charge(a, games * self.stride(a));
        self.queue.charge(b, games * self.stride(b));
        *self.games.entry(PairKey::new(a, b)).or_default() += games * self.weight(a, b);
        self.last.insert(a.to_string(), b.to_string());
        self.last.insert(b.to_string(), a.to_string());
    }

    fn mark_bot(&mut self, id: &str, bot: bool) {
        if bot {
            if !self.bots.contains(id) {
                self.bots.insert(id.to_string());
            }
        } else {
            self.bots.remove(id);
        }
    }

    fn stride(&self, id: &str) -> u32 {
        if self.bots.contains(id) { BOT_STRIDE } else { 1 }
    }

    fn weight(&self, a: &str, b: &str) -> u32 {
        1 << (u32::from(self.bots.contains(a)) + u32::from(self.bots.contains(b)))
    }

    fn played(&self, a: &str, b: &str) -> u32 {
        self.games.get(&PairKey::new(a, b)).copied().unwrap_or(0)
    }

    pub fn next_pair<'t>(
        &mut self,
        teams: &'t [Team],
        busy: &HashSet<String>,
        fits: impl Fn(&Team, &Team) -> bool,
        rating: impl Fn(&str) -> f64,
        rng: &mut impl Rng,
    ) -> Option<(&'t Team, &'t Team)> {
        for team in teams {
            self.mark_bot(&team.id, team.is_bot());
            self.queue.join(&team.id);
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
        let bot: Vec<bool> = teams.iter().map(|t| self.bots.contains(&t.id)).collect();
        let near = |i: usize, j: usize| nearest[i] == Some(j) || nearest[j] == Some(i);
        let eligible = |i: usize, j: usize| i != j && (gap(i, j) <= MAX_GAP || (near(i, j) && !(bot[i] && bot[j])));
        let index: HashMap<&str, usize> = teams.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();
        let free: Vec<(u32, usize)> = self
            .queue
            .iter()
            .filter_map(|(pass, id)| index.get(id).map(|&i| (pass, i)))
            .filter(|&(_, i)| !busy.contains(&teams[i].id))
            .collect();
        for tied in free.chunk_by(|x, y| x.0 == y.0) {
            let mut tied = tied.to_vec();
            tied.shuffle(rng);
            for (_, i) in tied {
                if let Some(j) = self.opponent(i, teams, busy, eligible, &fits, rng) {
                    let (a, b) = (&teams[i], &teams[j]);
                    return Some(if rng.random() { (a, b) } else { (b, a) });
                }
            }
        }
        None
    }

    fn opponent(
        &self,
        i: usize,
        teams: &[Team],
        busy: &HashSet<String>,
        eligible: impl Fn(usize, usize) -> bool,
        fits: impl Fn(&Team, &Team) -> bool,
        rng: &mut impl Rng,
    ) -> Option<usize> {
        let me = &teams[i].id;
        let rivals: Vec<usize> = (0..teams.len()).filter(|&j| eligible(i, j)).collect();
        let only_choice = rivals.len() == 1;
        let last = self.last.get(me);
        let open: Vec<(usize, (u32, u32))> = rivals
            .iter()
            .copied()
            .filter(|&j| only_choice || last != Some(&teams[j].id))
            .filter(|&j| !busy.contains(&teams[j].id) && fits(&teams[i], &teams[j]))
            .map(|j| (j, (self.queue.pass(&teams[j].id), self.played(me, &teams[j].id))))
            .collect();
        let least_open = open.iter().map(|&(_, (_, played))| played).min();
        let awaits_student = !self.bots.contains(me)
            && rivals.iter().any(|&j| {
                let rival = &teams[j].id;
                busy.contains(rival)
                    && !self.bots.contains(rival)
                    && last != Some(rival)
                    && self.queue.pass(rival) + 2 >= self.queue.pass(me)
                    && least_open.is_none_or(|least| self.played(me, rival) < least)
            });
        if awaits_student {
            return None;
        }
        let best_key = open.iter().map(|&(_, key)| key).min()?;
        let best: Vec<usize> = open
            .into_iter()
            .filter(|&(_, key)| key == best_key)
            .map(|(j, _)| j)
            .collect();
        best.choose(rng).copied()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};
    use std::time::{Duration, Instant};

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use isolation::board::Color;
    use isolation::record::{GameResult, InvalidCounts, Player, RECORD_VERSION, Reason};

    use crate::config::CommandTemplate;

    fn team(id: &str) -> Team {
        Team {
            id: id.into(),
            cmd: CommandTemplate::default(),
            build: Some(format!("{id}.jar").into()),
            anchor: false,
            multithread: false,
            remote: None,
        }
    }

    fn bot(id: &str) -> Team {
        Team {
            build: None,
            ..team(id)
        }
    }

    fn many(prefix: &str, n: usize, make: fn(&str) -> Team) -> Vec<Team> {
        (0..n).map(|i| make(&format!("{prefix}{i:03}"))).collect()
    }

    fn flat(_: &str) -> f64 {
        1500.0
    }

    fn sorted(pair: &[String; 2]) -> [String; 2] {
        let mut pair = pair.clone();
        pair.sort_unstable();
        pair
    }

    fn pair(a: &str, b: &str) -> [String; 2] {
        [a.into(), b.into()]
    }

    fn play(
        scheduler: &mut Scheduler,
        teams: &[Team],
        busy: &HashSet<String>,
        rating: impl Fn(&str) -> f64,
        rng: &mut StdRng,
    ) -> Option<[String; 2]> {
        let (x, y) = scheduler.next_pair(teams, busy, |_, _| true, rating, rng)?;
        let pair = [x.id.clone(), y.id.clone()];
        scheduler.record(&pair[0], &pair[1], 2);
        Some(pair)
    }

    fn play_rounds(scheduler: &mut Scheduler, teams: &[Team], rounds: usize, rng: &mut StdRng) -> Vec<[String; 2]> {
        (0..rounds)
            .map(|_| play(scheduler, teams, &HashSet::new(), flat, rng).unwrap())
            .collect()
    }

    /// Parties jouées par équipe, avec `slots` parties simultanées terminées dans l'ordre de départ.
    fn simulate(
        scheduler: &mut Scheduler,
        teams: &[Team],
        slots: usize,
        launches: usize,
        rng: &mut StdRng,
    ) -> HashMap<String, u32> {
        let mut running: VecDeque<[String; 2]> = VecDeque::new();
        let mut games = HashMap::new();
        let mut started = 0;
        while started < launches {
            let busy: HashSet<String> = running.iter().flatten().cloned().collect();
            let next = (running.len() < slots)
                .then(|| play(scheduler, teams, &busy, flat, rng))
                .flatten();
            match next {
                Some(pair) => {
                    for id in &pair {
                        *games.entry(id.clone()).or_default() += 2;
                    }
                    running.push_back(pair);
                    started += 1;
                }
                None => {
                    running
                        .pop_front()
                        .expect("aucune paire possible avec tous les slots libres");
                }
            }
        }
        games
    }

    fn past_game(red: &str, black: &str, bots: &[&str]) -> GameRecord {
        let player = |team: &str| Player {
            team: team.into(),
            build: (!bots.contains(&team)).then(|| format!("{team}-build")),
            anchor: false,
            remote: false,
        };
        GameRecord {
            id: format!("20261024T120000.000Z-{red}-{black}"),
            session: "s".into(),
            ts: "2026-10-24T12:00:00.000Z".into(),
            red: player(red),
            black: player(black),
            tournament: None,
            result: GameResult {
                v: RECORD_VERSION,
                time_ms: 500,
                opening: vec![],
                moves: vec![],
                winner: Color::Red,
                reason: Reason::NoMoves,
                invalid: InvalidCounts::default(),
                duration_ms: 0,
                move_ms: vec![],
            },
        }
    }

    #[test]
    fn replay_forgets_leads_from_another_regime() {
        let bots = ["n1", "n2", "n3", "n4"];
        let early = ["s1", "n1", "n2", "n3"];
        let mut history = Vec::new();
        for _ in 0..60 {
            for (i, a) in early.iter().enumerate() {
                for b in &early[i + 1..] {
                    history.extend([past_game(a, b, &bots), past_game(b, a, &bots)]);
                }
            }
        }
        for _ in 0..120 {
            history.extend(early.map(|b| past_game("s2", b, &bots)));
        }
        history.extend((0..100).map(|i| past_game("n4", early[i % 4], &bots)));

        let mut scheduler = Scheduler::from_history(&history, "s");
        let teams = [team("s1"), team("s2"), bot("n1"), bot("n2"), bot("n3"), bot("n4")];
        let mut rng = StdRng::seed_from_u64(3);
        let first = play_rounds(&mut scheduler, &teams, 10, &mut rng);
        for late in ["s2", "n4"] {
            assert!(
                first.iter().any(|p| p.contains(&late.to_string())),
                "{late} : {first:?}"
            );
        }
    }

    #[test]
    fn picks_the_least_played_opponent() {
        let teams = [team("a"), team("b"), team("c")];
        for seed in 0..20 {
            let mut scheduler = Scheduler::default();
            scheduler.record("a", "b", 4);
            scheduler.record("c", "a", 2);
            let mut rng = StdRng::seed_from_u64(seed);
            let first = play(&mut scheduler, &teams, &HashSet::new(), flat, &mut rng).unwrap();
            assert_eq!(sorted(&first), pair("b", "c"));
        }
    }

    #[test]
    fn needs_two_teams() {
        let mut rng = StdRng::seed_from_u64(3);
        assert!(
            Scheduler::default()
                .next_pair(&[team("a")], &HashSet::new(), |_, _| true, flat, &mut rng)
                .is_none()
        );
    }

    fn drawn(teams: &[Team], rating: impl Fn(&str) -> f64) -> BTreeSet<[String; 2]> {
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        (0..50)
            .map(|_| sorted(&play(&mut scheduler, teams, &HashSet::new(), &rating, &mut rng).unwrap()))
            .collect()
    }

    #[test]
    fn skips_pairs_too_far_apart() {
        let teams = [team("a"), team("b"), team("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1600.0,
            _ => 2500.0,
        };
        assert_eq!(drawn(&teams, rating), [pair("a", "b"), pair("b", "c")].into());
    }

    #[test]
    fn isolated_team_still_plays_its_nearest() {
        let teams = [team("a"), team("b"), team("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1550.0,
            _ => 3000.0,
        };
        assert!(drawn(&teams, rating).contains(&pair("b", "c")));
    }

    #[test]
    fn isolated_student_plays_its_nearest_bot() {
        let teams = [bot("a"), bot("b"), team("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1550.0,
            _ => 3000.0,
        };
        assert!(drawn(&teams, rating).contains(&pair("b", "c")));
    }

    #[test]
    fn isolated_bot_waits() {
        let teams = [team("a"), bot("b"), bot("c")];
        let rating = |id: &str| match id {
            "a" => 1500.0,
            "b" => 1600.0,
            _ => 3000.0,
        };
        assert!(drawn(&teams, rating).iter().all(|p| !p.contains(&"c".to_string())));
        let bots = [bot("a"), bot("b"), bot("c")];
        assert_eq!(drawn(&bots, rating), [pair("a", "b")].into());
    }

    #[test]
    fn busy_teams_are_left_out() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let busy = HashSet::from(["a".to_string(), "b".to_string()]);
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        let next = play(&mut scheduler, &teams, &busy, flat, &mut rng).unwrap();
        assert_eq!(sorted(&next), pair("c", "d"));
    }

    #[test]
    fn pairs_without_a_free_slot_are_left_out() {
        let teams = [team("mt"), team("b"), team("c")];
        let ordinary = |x: &Team, y: &Team| x.id != "mt" && y.id != "mt";
        let mut scheduler = Scheduler::default();
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let (x, y) = scheduler
                .next_pair(&teams, &HashSet::new(), ordinary, flat, &mut rng)
                .unwrap();
            assert_eq!(sorted(&[x.id.clone(), y.id.clone()]), pair("b", "c"));
        }
    }

    #[test]
    fn free_teams_do_not_replay_back_to_back() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let busy = HashSet::from(["c".to_string(), "d".to_string()]);
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        let first = play(&mut scheduler, &teams, &busy, flat, &mut rng).unwrap();
        assert_eq!(sorted(&first), pair("a", "b"));
        assert!(play(&mut scheduler, &teams, &busy, flat, &mut rng).is_none());
    }

    #[test]
    fn two_teams_replay_each_other() {
        let teams = [team("a"), team("b")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        play_rounds(&mut scheduler, &teams, 3, &mut rng);
    }

    #[test]
    fn students_meet_whatever_the_slot_count() {
        let teams = [many("s", 2, team), many("e", 4, bot)].concat();
        for slots in 1..=3 {
            let mut scheduler = Scheduler::default();
            let mut rng = StdRng::seed_from_u64(3);
            let games = simulate(&mut scheduler, &teams, slots, 300, &mut rng);
            let pvp = scheduler.played("s000", "s001");
            let student = games["s000"] + games["s001"];
            assert!(5 * 2 * pvp >= student, "{slots} slot(s) : {pvp} PvP sur {student}");
        }
    }

    #[test]
    fn no_pair_starves_with_two_slots() {
        let teams = [team("a"), team("b"), team("c"), team("d")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        simulate(&mut scheduler, &teams, 2, 60, &mut rng);
        let counts: Vec<u32> = scheduler.games.values().copied().collect();
        assert_eq!(counts.len(), 6);
        let spread = counts.iter().max().unwrap() - counts.iter().min().unwrap();
        assert!(spread <= 4, "{counts:?}");
    }

    #[test]
    fn late_team_catches_up_only_briefly() {
        let mut teams = many("t", 30, team);
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        play_rounds(&mut scheduler, &teams, 600, &mut rng);
        teams.push(team("new"));
        let next = play_rounds(&mut scheduler, &teams, 60, &mut rng);
        let with_new = |p: &[String; 2]| p.contains(&"new".to_string());
        let head_start = next.iter().take_while(|p| with_new(p)).count();
        let half = CATCH_UP as usize / 2;
        assert!((half..=half + 2).contains(&head_start), "{head_start}");
        let later = next[head_start..].iter().filter(|p| with_new(p)).count();
        assert!(later <= 6, "{later}");
    }

    #[test]
    fn slots_stay_busy_while_a_late_team_catches_up() {
        let mut teams = vec![team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        play_rounds(&mut scheduler, &teams, 30, &mut rng);
        teams.push(team("d"));
        let busy = HashSet::from(["d".to_string()]);
        let next = play(&mut scheduler, &teams, &busy, flat, &mut rng).unwrap();
        assert!(!next.contains(&"d".to_string()));
    }

    #[test]
    fn team_back_from_absence_catches_up_only_briefly() {
        let teams = many("t", 30, team);
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(5);
        play_rounds(&mut scheduler, &teams, 300, &mut rng);
        play_rounds(&mut scheduler, &teams[1..], 300, &mut rng);
        let next = play_rounds(&mut scheduler, &teams, 60, &mut rng);
        let back = |p: &[String; 2]| p.contains(&teams[0].id);
        let head_start = next.iter().take_while(|p| back(p)).count();
        let half = CATCH_UP as usize / 2;
        assert!((half..=half + 2).contains(&head_start), "{head_start}");
        let later = next[head_start..].iter().filter(|p| back(p)).count();
        assert!(later <= 6, "{later}");
    }

    #[test]
    fn bots_play_half_as_often() {
        let teams = [many("s", 4, team), many("e", 4, bot)].concat();
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        let games = simulate(&mut scheduler, &teams, 1, 1200, &mut rng);
        let total = |prefix: &str| {
            games
                .iter()
                .filter(|(id, _)| id.starts_with(prefix))
                .map(|(_, n)| n)
                .sum::<u32>()
        };
        let (students, bots) = (total("s"), total("e"));
        assert!(students.abs_diff(2 * bots) <= students / 10, "{students} {bots}");
        let kind = |p: &PairKey| u32::from(p.0.starts_with('e')) + u32::from(p.1.starts_with('e'));
        let mut by_kind = [0; 3];
        for (p, n) in &scheduler.games {
            by_kind[kind(p) as usize] += n / scheduler.weight(&p.0, &p.1);
        }
        assert!(by_kind[2] < by_kind[1] && by_kind[2] < by_kind[0], "{by_kind:?}");
    }

    #[test]
    fn fair_share_with_105_teams() {
        let teams = [many("s", 100, team), many("e", 5, bot)].concat();
        let mut scheduler = Scheduler::default();
        let mut rng = StdRng::seed_from_u64(3);
        let start = Instant::now();
        let launches = 2000u32;
        let games = simulate(&mut scheduler, &teams, 3, launches as usize, &mut rng);
        let per_draw = start.elapsed() / launches;
        let students: Vec<u32> = teams[..100]
            .iter()
            .map(|t| games.get(&t.id).copied().unwrap_or(0))
            .collect();
        let spread = students.iter().max().unwrap() - students.iter().min().unwrap();
        assert!(spread <= 4, "{spread}");
        assert!(per_draw < Duration::from_millis(20), "{per_draw:?}");
    }
}
