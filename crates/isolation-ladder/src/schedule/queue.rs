//! File équitable par temps virtuel (stride scheduling) : l'équipe au plus petit `pass` passe en
//! premier, et chaque partie l'avance de son stride.

use std::collections::{BTreeSet, HashMap};

pub struct FairQueue {
    order: BTreeSet<(u32, String)>,
    pass: HashMap<String, u32>,
    clock: u32,
    catch_up: u32,
}

impl FairQueue {
    pub fn new(catch_up: u32) -> Self {
        Self {
            order: BTreeSet::new(),
            pass: HashMap::new(),
            clock: 0,
            catch_up,
        }
    }

    const fn floor(&self) -> u32 {
        self.clock.saturating_sub(self.catch_up)
    }

    pub fn pass(&self, id: &str) -> u32 {
        self.pass.get(id).copied().unwrap_or_else(|| self.floor())
    }

    pub fn join(&mut self, id: &str) {
        if !self.pass.contains_key(id) {
            self.set(id, self.floor());
        }
    }

    pub fn charge(&mut self, id: &str, units: u32) {
        self.set(id, self.pass(id) + units);
    }

    pub fn advance(&mut self, to: u32) {
        self.clock = self.clock.max(to);
        let floor = self.floor();
        while let Some((_, id)) = self.order.first().filter(|(pass, _)| *pass < floor).cloned() {
            self.set(&id, floor);
        }
    }

    pub fn forget_leads(&mut self) {
        let leaders: Vec<String> = self
            .order
            .range((self.clock.saturating_add(1), String::new())..)
            .map(|(_, id)| id.clone())
            .collect();
        for id in leaders {
            self.set(&id, self.clock);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, &str)> {
        self.order.iter().map(|(pass, id)| (*pass, id.as_str()))
    }

    fn set(&mut self, id: &str, pass: u32) {
        if let Some(old) = self.pass.get_mut(id) {
            let entry = (std::mem::replace(old, pass), id.to_string());
            self.order.remove(&entry);
            self.order.insert((pass, entry.1));
        } else {
            self.pass.insert(id.to_string(), pass);
            self.order.insert((pass, id.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};

    use super::*;

    fn check(queue: &FairQueue) {
        assert_eq!(queue.order.len(), queue.pass.len());
        for (pass, id) in &queue.order {
            assert_eq!(queue.pass.get(id), Some(pass));
            assert!(*pass >= queue.floor());
        }
    }

    fn ids(queue: &FairQueue) -> Vec<&str> {
        queue.iter().map(|(_, id)| id).collect()
    }

    #[test]
    fn join_enters_at_floor() {
        let mut queue = FairQueue::new(10);
        queue.charge("a", 30);
        queue.advance(30);
        queue.join("b");
        assert_eq!(queue.pass("b"), 20);
    }

    #[test]
    fn join_keeps_a_known_team() {
        let mut queue = FairQueue::new(10);
        queue.charge("a", 5);
        queue.join("a");
        assert_eq!(queue.pass("a"), 5);
    }

    #[test]
    fn charge_reorders() {
        let mut queue = FairQueue::new(10);
        queue.join("a");
        queue.join("b");
        queue.charge("a", 5);
        assert_eq!(ids(&queue), ["b", "a"]);
    }

    #[test]
    fn advance_lifts_laggards_to_floor() {
        let mut queue = FairQueue::new(10);
        queue.join("a");
        queue.charge("b", 50);
        queue.advance(50);
        assert_eq!(queue.pass("a"), 40);
        check(&queue);
    }

    #[test]
    fn clock_never_goes_back() {
        let mut queue = FairQueue::new(10);
        queue.advance(30);
        queue.advance(10);
        queue.join("a");
        assert_eq!(queue.pass("a"), 20);
    }

    #[test]
    fn forget_leads_brings_leaders_back_to_clock() {
        let mut queue = FairQueue::new(10);
        queue.charge("ahead", 90);
        queue.charge("level", 50);
        queue.advance(50);
        queue.join("late");
        queue.forget_leads();
        assert_eq!(
            [queue.pass("ahead"), queue.pass("level"), queue.pass("late")],
            [50, 50, 40]
        );
        check(&queue);
    }

    #[test]
    fn index_and_order_stay_in_sync() {
        let mut queue = FairQueue::new(10);
        let mut rng = StdRng::seed_from_u64(3);
        let teams = ["a", "b", "c", "d", "e"];
        for _ in 0..500 {
            let id = teams[rng.random_range(0..teams.len())];
            match rng.random_range(0..3) {
                0 => queue.join(id),
                1 => queue.charge(id, rng.random_range(1..5)),
                _ => queue.advance(queue.pass(id)),
            }
            check(&queue);
        }
    }
}
