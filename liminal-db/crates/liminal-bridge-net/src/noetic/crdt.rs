use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::peers::PeerId;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hlc {
    pub phys_ms: u64,
    pub log: u32,
    pub node: PeerId,
}

impl Hlc {
    pub fn new(node: PeerId, phys_ms: u64) -> Self {
        Hlc {
            phys_ms,
            log: 0,
            node,
        }
    }

    pub fn tick(&mut self, now_ms: u64) {
        if now_ms > self.phys_ms {
            self.phys_ms = now_ms;
            self.log = 0;
        } else {
            self.log = self.log.saturating_add(1);
        }
    }

    pub fn merged(&self, other: &Hlc, node: PeerId, now_ms: u64) -> Hlc {
        let max_phys = self.phys_ms.max(other.phys_ms).max(now_ms);
        let log = if max_phys == self.phys_ms && max_phys == other.phys_ms {
            self.log.max(other.log) + 1
        } else if max_phys == self.phys_ms {
            self.log + 1
        } else if max_phys == other.phys_ms {
            other.log + 1
        } else {
            0
        };
        Hlc {
            phys_ms: max_phys,
            log,
            node,
        }
    }

    pub fn compare(&self, other: &Hlc, ctx: &MergeContext) -> Ordering {
        match self.phys_ms.cmp(&other.phys_ms) {
            Ordering::Equal => match self.log.cmp(&other.log) {
                Ordering::Equal => match ctx
                    .credence_for(&self.node)
                    .partial_cmp(&ctx.credence_for(&other.node))
                {
                    Some(Ordering::Equal) | None => self.node.cmp(&other.node),
                    Some(order) => order,
                },
                other => other,
            },
            other => other,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lww<T> {
    pub value: T,
    pub clock: Hlc,
}

impl<T> Lww<T> {
    pub fn new(value: T, clock: Hlc) -> Self {
        Lww { value, clock }
    }

    pub fn merge(&mut self, other: &Lww<T>, ctx: &MergeContext)
    where
        T: Clone,
    {
        if self.clock.compare(&other.clock, ctx) == Ordering::Less {
            self.value = other.value.clone();
            self.clock = other.clock;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(bound = "T: Ord + Serialize + for<'a> Deserialize<'a>")]
pub struct OrSet<T> {
    adds: BTreeMap<T, Hlc>,
    removals: BTreeMap<T, Hlc>,
}

impl<T> Default for OrSet<T>
where
    T: Ord,
{
    fn default() -> Self {
        OrSet {
            adds: BTreeMap::new(),
            removals: BTreeMap::new(),
        }
    }
}

impl<T> OrSet<T>
where
    T: Ord + Clone,
{
    pub fn insert(&mut self, value: T, clock: Hlc, ctx: &MergeContext) {
        match self.adds.get(&value) {
            Some(existing) if existing.compare(&clock, ctx) != Ordering::Less => {}
            _ => {
                self.adds.insert(value, clock);
            }
        }
    }

    pub fn remove(&mut self, value: T, clock: Hlc, ctx: &MergeContext) {
        match self.removals.get(&value) {
            Some(existing) if existing.compare(&clock, ctx) != Ordering::Less => {}
            _ => {
                self.removals.insert(value, clock);
            }
        }
    }

    pub fn elements(&self, ctx: &MergeContext) -> Vec<T> {
        self.adds
            .iter()
            .filter(|(value, add_clock)| match self.removals.get(*value) {
                Some(rem_clock) => add_clock.compare(rem_clock, ctx) == Ordering::Greater,
                None => true,
            })
            .map(|(value, _)| value.clone())
            .collect()
    }

    pub fn merge(&mut self, other: &OrSet<T>, ctx: &MergeContext) {
        for (value, clock) in &other.adds {
            self.insert(value.clone(), *clock, ctx);
        }
        for (value, clock) in &other.removals {
            self.remove(value.clone(), *clock, ctx);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct PnCounter {
    positives: BTreeMap<PeerId, i64>,
    negatives: BTreeMap<PeerId, i64>,
}

impl PnCounter {
    pub fn increment(&mut self, peer: PeerId, delta: i64) {
        *self.positives.entry(peer).or_default() += delta.abs();
    }

    pub fn decrement(&mut self, peer: PeerId, delta: i64) {
        *self.negatives.entry(peer).or_default() += delta.abs();
    }

    pub fn value(&self) -> i64 {
        let pos: i64 = self.positives.values().sum();
        let neg: i64 = self.negatives.values().sum();
        pos - neg
    }

    pub fn merge(&mut self, other: &PnCounter) {
        for (peer, value) in &other.positives {
            let entry = self.positives.entry(*peer).or_default();
            *entry = (*entry).max(*value);
        }
        for (peer, value) in &other.negatives {
            let entry = self.negatives.entry(*peer).or_default();
            *entry = (*entry).max(*value);
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MergeContext {
    credence: BTreeMap<PeerId, f32>,
    default: f32,
}

impl MergeContext {
    pub fn new(default: f32) -> Self {
        MergeContext {
            credence: BTreeMap::new(),
            default,
        }
    }

    pub fn with_peer(mut self, peer: PeerId, credence: f32) -> Self {
        self.credence.insert(peer, credence);
        self
    }

    pub fn set_peer(&mut self, peer: PeerId, credence: f32) {
        self.credence.insert(peer, credence);
    }

    pub fn credence_for(&self, peer: &PeerId) -> f32 {
        self.credence.get(peer).copied().unwrap_or(self.default)
    }
}

impl Default for MergeContext {
    fn default() -> Self {
        MergeContext::new(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::PeerId;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn ctx(default: f32) -> MergeContext {
        MergeContext::new(default)
    }

    #[test]
    fn hlc_merges_monotonically() {
        let key_a = SigningKey::generate(&mut OsRng);
        let key_b = SigningKey::generate(&mut OsRng);
        let mut hlc_a = Hlc::new(PeerId::from_keypair(&key_a), 1_000);
        let hlc_b = Hlc::new(PeerId::from_keypair(&key_b), 1_500);
        hlc_a.tick(1_100);
        let merged = hlc_a.merged(&hlc_b, hlc_a.node, 1_600);
        assert!(merged.phys_ms >= hlc_a.phys_ms);
        assert!(merged.phys_ms >= hlc_b.phys_ms);
    }

    #[test]
    fn orset_converges_after_merges() {
        let key_a = SigningKey::generate(&mut OsRng);
        let key_b = SigningKey::generate(&mut OsRng);
        let mut set_a = OrSet::<String>::default();
        let mut set_b = OrSet::<String>::default();
        let peer_a = PeerId::from_keypair(&key_a);
        let peer_b = PeerId::from_keypair(&key_b);
        let ctx = ctx(0.5).with_peer(peer_a, 0.8).with_peer(peer_b, 0.7);

        set_a.insert("model/a".into(), Hlc::new(peer_a, 1_000), &ctx);
        set_b.insert("model/a".into(), Hlc::new(peer_b, 900), &ctx);
        set_b.remove("model/a".into(), Hlc::new(peer_b, 1_100), &ctx);

        set_a.merge(&set_b, &ctx);
        set_b.merge(&set_a, &ctx);

        assert!(set_a.elements(&ctx).is_empty());
        assert_eq!(set_a.elements(&ctx), set_b.elements(&ctx));
    }

    #[test]
    fn pn_counter_converges() {
        let key_a = SigningKey::generate(&mut OsRng);
        let key_b = SigningKey::generate(&mut OsRng);
        let peer_a = PeerId::from_keypair(&key_a);
        let peer_b = PeerId::from_keypair(&key_b);
        let mut counter_a = PnCounter::default();
        let mut counter_b = PnCounter::default();

        counter_a.increment(peer_a, 5);
        counter_b.increment(peer_b, 3);
        counter_b.decrement(peer_b, 1);

        counter_a.merge(&counter_b);
        counter_b.merge(&counter_a);

        assert_eq!(counter_a.value(), counter_b.value());
        assert_eq!(counter_a.value(), 7);
    }
}
