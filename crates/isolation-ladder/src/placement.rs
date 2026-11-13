//! Où se joue chaque paire : les slots du serveur principal (`[[slot]]`) et ceux des nœuds
//! connectés (`node/`). Une paire tient un bail ([`Lease`]) sur un slot concret pour ses deux parties
//! et sa pause ; chaque partie occupe en plus le slot ([`InUse`]) le temps de se jouer, et un smoke
//! test peut s'y glisser entre deux parties, comme avant les nœuds.

use std::sync::{Arc, Condvar, Mutex, PoisonError};

use crate::config::Config;
use crate::sync::lock;

/// Nœud du serveur principal, celui des `[[slot]]` de `ladder.toml`.
pub const LOCAL: &str = "local";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotKind {
    Ordinary,
    Multithread,
    /// Slot d'un nœud : seules les paires portables y jouent.
    Portable,
}

/// Ce qu'une paire demande.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PairClass {
    /// Partie ordinaire qui doit rester au serveur principal.
    Heavy,
    Multithread,
    /// Les deux joueurs sont légers : un nœud d'abord, sinon un slot ordinaire.
    Portable,
}

impl SlotKind {
    pub const fn accepts(self, class: PairClass) -> bool {
        matches!(
            (self, class),
            (Self::Ordinary, PairClass::Heavy | PairClass::Portable)
                | (Self::Multithread, PairClass::Multithread)
                | (Self::Portable, PairClass::Portable)
        )
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SlotId {
    pub node: Arc<str>,
    /// Index du slot dans son nœud : celui de `[[slot]]` au serveur principal.
    pub index: usize,
}

impl SlotId {
    pub fn local(&self) -> bool {
        self.node.as_ref() == LOCAL
    }
}

struct Slot {
    id: SlotId,
    kind: SlotKind,
    leased: bool,
    busy: bool,
    /// Son nœud est parti : plus de nouveau bail, l'entrée attend que son bail soit rendu.
    gone: bool,
}

impl Slot {
    const fn new(id: SlotId, kind: SlotKind) -> Self {
        Self {
            id,
            kind,
            leased: false,
            busy: false,
            gone: false,
        }
    }

    const fn free(&self) -> bool {
        !self.leased && !self.gone
    }
}

pub struct Placement {
    slots: Mutex<Vec<Slot>>,
    changed: Condvar,
}

impl Placement {
    pub fn new(cfg: &Config) -> Arc<Self> {
        let local: Arc<str> = Arc::from(LOCAL);
        let slots = (0..cfg.slots())
            .map(|index| {
                let kind = if cfg.multithread_slot(index) {
                    SlotKind::Multithread
                } else {
                    SlotKind::Ordinary
                };
                Slot::new(
                    SlotId {
                        node: Arc::clone(&local),
                        index,
                    },
                    kind,
                )
            })
            .collect();
        Arc::new(Self {
            slots: Mutex::new(slots),
            changed: Condvar::new(),
        })
    }

    /// Classe d'une paire non portable. Faute de slot du genre voulu au serveur principal, l'autre :
    /// aucune partie n'attend un slot qui n'existe pas.
    pub fn class(&self, multithread: bool) -> PairClass {
        let slots = lock(&self.slots);
        let has = |kind| slots.iter().any(|s| s.id.local() && s.kind == kind);
        let multithread = if multithread {
            has(SlotKind::Multithread) || !has(SlotKind::Ordinary)
        } else {
            !has(SlotKind::Ordinary) && has(SlotKind::Multithread)
        };
        if multithread {
            PairClass::Multithread
        } else {
            PairClass::Heavy
        }
    }

    pub fn fits(&self, class: PairClass) -> bool {
        lock(&self.slots).iter().any(|s| s.free() && s.kind.accepts(class))
    }

    /// Aucun bail en cours, pause comprise.
    pub fn idle(&self) -> bool {
        !lock(&self.slots).iter().any(|s| s.leased)
    }

    /// Le slot libre qui convient : un nœud d'abord pour une paire portable, puis le plus petit index.
    pub fn try_lease(self: &Arc<Self>, class: PairClass) -> Option<Lease> {
        let mut slots = lock(&self.slots);
        let position = (0..slots.len())
            .filter(|&i| slots[i].free() && slots[i].kind.accepts(class))
            .min_by_key(|&i| (slots[i].kind != SlotKind::Portable, slots[i].id.index, i))?;
        slots[position].leased = true;
        Some(Lease {
            placement: Arc::clone(self),
            position,
            id: slots[position].id.clone(),
            kind: slots[position].kind,
        })
    }

    /// Occupe le slot de `lease` le temps d'une partie ; sans bail (smoke test), un slot du serveur
    /// principal qui accepte `class`, loué ou non. Attend qu'il soit libre.
    pub fn occupy(self: &Arc<Self>, lease: Option<&Lease>, class: PairClass) -> InUse {
        let usable = |slots: &[Slot], i: usize| {
            let slot: &Slot = &slots[i];
            let right = lease.map_or_else(|| slot.id.local() && slot.kind.accepts(class), |l| i == l.position);
            right && !slot.busy
        };
        let slots = lock(&self.slots);
        let mut slots = self
            .changed
            .wait_while(slots, |slots| !(0..slots.len()).any(|i| usable(slots, i)))
            .unwrap_or_else(PoisonError::into_inner);
        let Some(position) = (0..slots.len()).find(|&i| usable(&slots, i)) else {
            unreachable!("attendu tant qu'aucun slot n'était libre");
        };
        slots[position].busy = true;
        InUse {
            placement: Arc::clone(self),
            position,
            id: slots[position].id.clone(),
        }
    }

    /// Ajoute les `count` slots portables d'un nœud prêt.
    pub fn add_node(&self, node: &str, count: usize) {
        let node: Arc<str> = Arc::from(node);
        let mut slots = lock(&self.slots);
        for index in 0..count {
            let slot = Slot::new(
                SlotId {
                    node: Arc::clone(&node),
                    index,
                },
                SlotKind::Portable,
            );
            // Une entrée partie n'est réutilisée qu'une fois son bail et sa partie rendus.
            match slots.iter().position(|s| s.gone && !s.leased && !s.busy) {
                Some(reused) => slots[reused] = slot,
                None => slots.push(slot),
            }
        }
        self.changed.notify_all();
    }

    /// Retire les slots d'un nœud perdu ou qui n'est plus prêt ; ses baux en cours meurent.
    pub fn remove_node(&self, node: &str) {
        let mut slots = lock(&self.slots);
        for slot in slots.iter_mut().filter(|s| s.id.node.as_ref() == node) {
            slot.gone = true;
        }
        self.changed.notify_all();
    }

    fn release(&self, position: usize, release: impl FnOnce(&mut Slot)) {
        release(&mut lock(&self.slots)[position]);
        self.changed.notify_all();
    }
}

/// Une paire (ou un match de tournoi) tient ce slot ; le rendre le libère.
pub struct Lease {
    placement: Arc<Placement>,
    position: usize,
    id: SlotId,
    kind: SlotKind,
}

impl Lease {
    pub const fn slot(&self) -> &SlotId {
        &self.id
    }

    pub const fn kind(&self) -> SlotKind {
        self.kind
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.placement.release(self.position, |slot| slot.leased = false);
    }
}

pub struct InUse {
    placement: Arc<Placement>,
    position: usize,
    id: SlotId,
}

impl InUse {
    pub const fn slot(&self) -> &SlotId {
        &self.id
    }

    pub const fn index(&self) -> usize {
        self.id.index
    }
}

impl Drop for InUse {
    fn drop(&mut self) {
        self.placement.release(self.position, |slot| slot.busy = false);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::config::Config;

    fn placement(slots: &str) -> Arc<Placement> {
        let cfg: Config = format!("referee = [\"r\"]\ntime_ms = 100\nsession = \"s\"\n{slots}")
            .parse()
            .unwrap();
        Placement::new(&cfg)
    }

    fn at(lease: &Lease) -> (&str, usize) {
        (&lease.slot().node, lease.slot().index)
    }

    #[test]
    fn portable_pairs_prefer_node_slots_then_the_main_server() {
        let p = placement("[[slot]]\n[[slot]]\n");
        p.add_node("node-a", 1);
        let first = p.try_lease(PairClass::Portable).unwrap();
        assert_eq!(at(&first), ("node-a", 0));
        let second = p.try_lease(PairClass::Portable).unwrap();
        assert_eq!(at(&second), (LOCAL, 0));
        drop(first);
        assert_eq!(at(&p.try_lease(PairClass::Portable).unwrap()), ("node-a", 0));
    }

    #[test]
    fn heavy_pairs_never_take_a_portable_slot() {
        let p = placement("[[slot]]\n");
        p.add_node("node-a", 2);
        let main_slot = p.try_lease(PairClass::Heavy).unwrap();
        assert_eq!(at(&main_slot), (LOCAL, 0));
        assert!(p.try_lease(PairClass::Heavy).is_none());
        assert!(!p.fits(PairClass::Heavy));
        assert!(p.fits(PairClass::Portable));
    }

    #[test]
    fn multithread_pairs_take_only_multithread_slots() {
        let p = placement("[[slot]]\n[[slot]]\nmultithread = true\n");
        p.add_node("node-a", 1);
        let multi = p.try_lease(PairClass::Multithread).unwrap();
        assert_eq!(at(&multi), (LOCAL, 1));
        assert_eq!(multi.kind(), SlotKind::Multithread);
        assert!(p.try_lease(PairClass::Multithread).is_none());
    }

    #[test]
    fn a_dropped_lease_frees_its_slot_and_idle_follows_the_leases() {
        let p = placement("[[slot]]\n");
        assert!(p.idle());
        let lease = p.try_lease(PairClass::Heavy).unwrap();
        assert!(!p.idle() && !p.fits(PairClass::Heavy));
        drop(lease);
        assert!(p.idle() && p.fits(PairClass::Heavy));
    }

    #[test]
    fn removing_a_node_kills_its_leases_and_slots() {
        let p = placement("");
        p.add_node("node-a", 1);
        let lease = p.try_lease(PairClass::Portable).unwrap();
        p.remove_node("node-a");
        assert!(!p.idle(), "le bail mort est encore tenu");
        assert_eq!(
            at(&p.try_lease(PairClass::Portable).unwrap()),
            (LOCAL, 0),
            "plus de slot sur node-a"
        );

        drop(lease);
        assert!(p.idle());
        assert_eq!(at(&p.try_lease(PairClass::Portable).unwrap()), (LOCAL, 0));
        p.add_node("node-a", 1);
        assert_eq!(at(&p.try_lease(PairClass::Portable).unwrap()), ("node-a", 0));
    }

    #[test]
    fn a_game_waits_for_its_slot_while_a_smoke_test_uses_it() {
        let p = placement("[[slot]]\n");
        let lease = p.try_lease(PairClass::Heavy).unwrap();
        let smoke = p.occupy(None, PairClass::Heavy);
        assert_eq!(smoke.index(), 0);
        thread::scope(|scope| {
            let (tx, rx) = mpsc::channel();
            let (p, lease) = (&p, &lease);
            scope.spawn(move || tx.send(p.occupy(Some(lease), PairClass::Heavy).index()).unwrap());
            assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
            drop(smoke);
            assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(0));
        });
    }

    #[test]
    fn smoke_tests_use_a_main_server_slot_of_their_kind_even_if_leased() {
        let p = placement("[[slot]]\n[[slot]]\nmultithread = true\n");
        p.add_node("node-a", 1);
        let _lease = p.try_lease(PairClass::Multithread).unwrap();
        assert_eq!(p.occupy(None, PairClass::Multithread).index(), 1);
        assert_eq!(p.occupy(None, PairClass::Portable).index(), 0);
    }

    #[test]
    fn without_any_multithread_slot_a_multithread_pair_is_heavy() {
        let ordinary = placement("[[slot]]\n");
        assert_eq!(ordinary.class(true), PairClass::Heavy);
        let only_multi = placement("[[slot]]\nmultithread = true\n");
        assert_eq!(only_multi.class(false), PairClass::Multithread);
        assert_eq!(placement("").class(false), PairClass::Heavy);
    }
}
