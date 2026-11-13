//! Paires du ladder : slots occupés, équipes prises et paires annoncées pendant la pause d'un slot.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use rand::Rng;

use isolation_core::notation;
use isolation_core::rules::random_opening;

use crate::arena::Arena;
use crate::config::Config;
use crate::placement::{Lease, PairClass, Placement};
use crate::play::{Report, Rest, spawn_game, spawn_pair};
use crate::roster::{Team, pair_class};
use crate::schedule::{Announcements, Scheduler};
use crate::tournament::Organizer;

pub struct Pairs {
    pub scheduler: Scheduler,
    /// Slots et baux : une paire en cours, pause comprise, tient le sien.
    placement: Arc<Placement>,
    /// Équipes qui peuvent jouer sur un nœud, relues par la boucle à chaque tour.
    portable: HashSet<String>,
    busy: HashSet<String>,
    announced: Announcements,
    pub round: u64,
}

impl Pairs {
    pub fn new(scheduler: Scheduler, placement: Arc<Placement>) -> Self {
        Self {
            scheduler,
            placement,
            portable: HashSet::new(),
            busy: HashSet::new(),
            announced: Announcements::default(),
            round: 0,
        }
    }

    pub fn idle(&self) -> bool {
        self.placement.idle()
    }

    /// Classe de la paire : où elle a le droit de jouer.
    pub fn class(&self, a: &Team, b: &Team) -> PairClass {
        pair_class(&self.placement, &self.portable, a, b)
    }

    pub fn set_portable(&mut self, portable: HashSet<String>) {
        self.portable = portable;
    }

    /// La prochaine paire qui tient dans un slot libre.
    pub fn next_pair<'t>(
        &mut self,
        teams: &'t [Team],
        unavailable: &HashSet<String>,
        rating: impl Fn(&str) -> f64,
        rng: &mut impl Rng,
    ) -> Option<(&'t Team, &'t Team)> {
        let (placement, portable) = (&self.placement, &self.portable);
        let fits = |a: &Team, b: &Team| placement.fits(pair_class(placement, portable, a, b));
        self.scheduler.next_pair(teams, unavailable, fits, rating, rng)
    }

    /// Une paire de plus tient dans `rounds`, annonces comprises.
    pub fn open(&self, rounds: Option<u64>) -> bool {
        let announced = u64::try_from(self.announced.len()).unwrap_or(u64::MAX);
        rounds.is_none_or(|n| self.round.saturating_add(announced) < n)
    }

    /// Équipes hors tirage. `resting` : la paire en pause dont le slot est à pourvoir, comptée libre.
    pub fn unavailable(&self, waiting: &HashSet<String>, resting: Option<&[String; 2]>) -> HashSet<String> {
        let busy = self
            .busy
            .iter()
            .filter(|id| resting.is_none_or(|pair| !pair.contains(id)));
        busy.chain(waiting)
            .cloned()
            .chain(self.announced.teams().map(String::from))
            .collect()
    }

    fn publish(&self, cfg: &Config, teams: &[Team]) {
        let queued = self.scheduler.publish(cfg.data_dir(), teams, &self.announced);
        queued.unwrap_or_else(|e| eprintln!("   file d'attente non publiée : {e}"));
    }

    /// Lance la paire si un slot l'accepte. Faux sinon (un nœud vient de partir, un tournoi a pris
    /// le slot).
    pub fn launch(
        &mut self,
        arena: &Arc<Arena>,
        teams: &[Team],
        [a, b]: [Team; 2],
        reports: &Sender<Report>,
        rng: &mut impl Rng,
    ) -> bool {
        let cfg = arena.config();
        let Some(lease) = self.placement.try_lease(self.class(&a, &b)) else {
            return false;
        };
        self.round += 1;
        self.scheduler.record(&a.id, &b.id, 2);
        self.busy.extend([a.id.clone(), b.id.clone()]);
        self.publish(cfg, teams);
        let opening = random_opening(cfg.opening_plies, rng);
        eprintln!(
            "== paire {} : {} contre {} sur {}/{} (ouverture {})",
            self.round,
            a.id,
            b.id,
            lease.slot().node,
            lease.slot().index,
            notation::join(&opening)
        );
        spawn_pair(Arc::clone(arena), lease, a, b, opening, reports.clone());
        true
    }

    /// Lance les matchs de tournoi prêts qui trouvent un slot libre.
    pub fn launch_tournament(&mut self, arena: &Arc<Arena>, tournaments: &mut Organizer, reports: &Sender<Report>) {
        let cfg = arena.config();
        for p in tournaments.pending(&self.busy) {
            if let Some(lease) = self.placement.try_lease(self.class(&p.red, &p.black)) {
                self.busy.extend([p.red.id.clone(), p.black.id.clone()]);
                tournaments.start(cfg, &p);
                spawn_game(Arc::clone(arena), lease, p, reports.clone());
            }
        }
    }

    /// Tire la paire qui prendra le slot de `rest` à la fin de sa pause, et l'annonce.
    pub fn announce(
        &mut self,
        cfg: &Config,
        teams: &[Team],
        waiting: &HashSet<String>,
        rest: Rest,
        rating: impl Fn(&str) -> f64,
        rng: &mut impl Rng,
    ) {
        let unavailable = self.unavailable(waiting, Some(&rest.pair));
        let (placement, portable) = (&self.placement, &self.portable);
        let fits = |a: &Team, b: &Team| rest.kind.accepts(pair_class(placement, portable, a, b));

        let Some((a, b)) = self.scheduler.next_pair(teams, &unavailable, fits, rating, rng) else {
            return;
        };
        eprintln!("   annonce : {} contre {} à {}", a.id, b.id, rest.at);
        let pair = [a.clone(), b.clone()];
        self.announced.add(rest.pair, pair, rest.at);
        self.publish(cfg, teams);
    }

    pub fn cancel_stale(&mut self, cfg: &Config, teams: &[Team], waiting: &HashSet<String>) {
        if self.announced.cancel_stale(teams, &self.busy, waiting) {
            eprintln!("   annonce annulée : nouveau jar, équipe absente ou tournoi");
            self.publish(cfg, teams);
        }
    }

    /// Lance les annonces dont la pause est finie, sauf si plus aucun slot ne les accepte (un
    /// tournoi l'a pris, son nœud est parti).
    pub fn launch_announced(
        &mut self,
        arena: &Arc<Arena>,
        teams: &[Team],
        reports: &Sender<Report>,
        rng: &mut impl Rng,
    ) {
        let cfg = arena.config();
        for pair in self.announced.take_ready() {
            let ids = [pair[0].id.clone(), pair[1].id.clone()];
            if !self.launch(arena, teams, pair, reports, rng) {
                eprintln!(
                    "   annonce {} contre {} annulée : plus de slot pour elle",
                    ids[0], ids[1]
                );
                self.publish(cfg, teams);
            }
        }
    }

    /// La paire a fini sa pause : son slot est libre, son annonce peut partir.
    pub fn done(&mut self, pair: &[String; 2], lease: Lease) {
        drop(lease);
        for id in pair {
            self.busy.remove(id);
        }
        self.announced.rested(pair);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use rand::SeedableRng;

    use super::*;
    use crate::roster::roster;

    #[test]
    fn the_resting_pair_counts_free_and_the_announcement_waits_for_its_slot() {
        let dir = std::env::temp_dir().join(format!("isolation-announce-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let teams = ["a", "b", "c", "d"]
            .map(|id| format!("[[team]]\nid = \"{id}\"\ncmd = [\"bot\"]\n"))
            .concat();
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\n{teams}",
            out = dir.join("games.jsonl")
        );
        let cfg: Config = toml.parse().unwrap();
        let teams = roster(&cfg, None, &mut HashSet::new());
        for seed in 0..20 {
            let placement = Placement::new(&cfg);
            let mut pairs = Pairs::new(Scheduler::default(), Arc::clone(&placement));
            pairs.scheduler.record("a", "b", 2);
            pairs.busy.extend(["a", "b", "c"].map(String::from));
            let resting = placement.try_lease(PairClass::Heavy).unwrap();
            assert!(!placement.fits(PairClass::Heavy), "le seul slot est en pause");

            let rest = Rest {
                pair: ["a".into(), "b".into()],
                kind: resting.kind(),
                at: "2026-11-05T12:01:00.000Z".into(),
            };
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            pairs.announce(&cfg, &teams, &HashSet::new(), rest, |_| 1500.0, &mut rng);
            let announced: Vec<&str> = pairs.announced.teams().collect();
            assert!(announced.contains(&"d") && !announced.contains(&"c"), "{announced:?}");
            let unavailable = pairs.unavailable(&HashSet::new(), None);
            assert!(announced.iter().all(|id| unavailable.contains(*id)));
            let file: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(dir.join("queue.json")).unwrap()).unwrap();
            assert_eq!(file["next"][0]["teams"], serde_json::json!(announced));
            assert!(pairs.announced.take_ready().is_empty());
            pairs.done(&["a".into(), "b".into()], resting);
            assert_eq!(pairs.announced.take_ready().len(), 1);
            assert!(pairs.idle());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_slot_withdrawn_during_rest_cancels_its_announcement() {
        let dir = std::env::temp_dir().join(format!("isolation-withdrawn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let teams = ["a", "b", "c", "d", "e"]
            .map(|id| format!("[[team]]\nid = \"{id}\"\ncmd = [\"bot\"]\nportable = true\n"))
            .concat();
        let toml = format!(
            "referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\nout = {out:?}\n[nodes]\n{teams}",
            out = dir.join("games.jsonl")
        );
        let cfg: Config = toml.parse().unwrap();
        let teams = roster(&cfg, None, &mut HashSet::new());
        let arena = Arc::new(Arena::new(cfg, crate::hub::Hub::new(|_, _, _| {})));
        let placement = Arc::clone(arena.placement());
        placement.add_node("node-a", 1);
        let mut pairs = Pairs::new(Scheduler::default(), Arc::clone(&placement));
        pairs.set_portable(teams.iter().map(|t| t.id.clone()).collect());
        let resting = placement.try_lease(PairClass::Portable).unwrap();
        assert_eq!(resting.slot().node.as_ref(), "node-a");
        let _main = placement.try_lease(PairClass::Heavy).unwrap();
        pairs.busy.extend(["a", "b", "e"].map(String::from));
        let rest = Rest {
            pair: ["a".into(), "b".into()],
            kind: resting.kind(),
            at: "2026-11-13T12:01:00.000Z".into(),
        };
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        pairs.announce(arena.config(), &teams, &HashSet::new(), rest, |_| 1500.0, &mut rng);
        assert_eq!(pairs.announced.len(), 1);

        placement.remove_node("node-a");
        pairs.done(&["a".into(), "b".into()], resting);
        let (reports, _received) = std::sync::mpsc::channel();
        pairs.launch_announced(&arena, &teams, &reports, &mut rng);
        assert_eq!(pairs.announced.len(), 0, "annonce tombée");
        assert_eq!(pairs.round, 0, "aucune paire lancée");
        let _ = fs::remove_dir_all(&dir);
    }
}
