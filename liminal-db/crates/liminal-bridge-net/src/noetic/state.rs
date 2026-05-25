use std::collections::BTreeMap;

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::crdt::{Hlc, Lww, MergeContext, OrSet};
use crate::peers::PeerId;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResonantModelHeader {
    pub id: String,
    pub version: Hlc,
    pub hash: String,
    pub source: PeerId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ResonantModelTopK {
    pub entries: Vec<String>,
    pub updated: Hlc,
}

impl ResonantModelTopK {
    pub fn new(entries: Vec<String>, updated: Hlc) -> Self {
        ResonantModelTopK { entries, updated }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EpochHeader {
    pub id: u64,
    pub version: Hlc,
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SeedHeader {
    pub id: String,
    pub epoch: u64,
    pub version: Hlc,
    pub source: PeerId,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum SeedStatus {
    Pending,
    Active { started_ms: u64 },
    Committed { finished_ms: u64 },
    Failed { error: String },
    Observed { note: String },
}

impl SeedStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeedStatus::Pending => "pending",
            SeedStatus::Active { .. } => "active",
            SeedStatus::Committed { .. } => "committed",
            SeedStatus::Failed { .. } => "failed",
            SeedStatus::Observed { .. } => "observed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PolicyBundle {
    pub version: Hlc,
    pub hash: String,
    pub payload: JsonValue,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoeticState {
    pub peer: PeerId,
    pub clock: Hlc,
    pub models: OrSet<ResonantModelHeader>,
    pub model_topk: Lww<ResonantModelTopK>,
    pub epochs: OrSet<EpochHeader>,
    pub seeds: OrSet<SeedHeader>,
    pub seed_status: Lww<SeedStatus>,
    pub policies: Lww<PolicyBundle>,
    pub credence: BTreeMap<PeerId, f32>,
}

impl NoeticState {
    pub fn new(peer: PeerId) -> Self {
        let clock = Hlc::new(peer, 0);
        let topk = ResonantModelTopK::new(Vec::new(), clock);
        let seed_status = SeedStatus::Pending;
        let default_policy = PolicyBundle {
            version: clock,
            hash: String::new(),
            payload: JsonValue::Null,
        };
        NoeticState {
            peer,
            clock,
            models: OrSet::default(),
            model_topk: Lww::new(topk, clock),
            epochs: OrSet::default(),
            seeds: OrSet::default(),
            seed_status: Lww::new(seed_status, clock),
            policies: Lww::new(default_policy, clock),
            credence: BTreeMap::new(),
        }
    }

    pub fn set_credence(&mut self, peer: PeerId, value: f32) {
        self.credence.insert(peer, value);
    }

    pub fn merge(&mut self, other: &NoeticState) {
        for (peer, value) in &other.credence {
            let entry = self.credence.entry(*peer).or_insert(*value);
            if *value > *entry {
                *entry = *value;
            }
        }

        let ctx = self.merge_context(Some(other));
        self.models.merge(&other.models, &ctx);
        self.epochs.merge(&other.epochs, &ctx);
        self.seeds.merge(&other.seeds, &ctx);
        self.model_topk.merge(&other.model_topk, &ctx);
        self.seed_status.merge(&other.seed_status, &ctx);
        self.policies.merge(&other.policies, &ctx);
        let now_ms = self.clock.phys_ms.max(other.clock.phys_ms);
        self.clock = self.clock.merged(&other.clock, self.peer, now_ms);
    }

    pub fn apply_model_header(&mut self, header: ResonantModelHeader) {
        let ctx = self.merge_context(None);
        let version = header.version;
        self.models.insert(header, version, &ctx);
    }

    pub fn apply_epoch(&mut self, epoch: EpochHeader) {
        let ctx = self.merge_context(None);
        let version = epoch.version;
        self.epochs.insert(epoch, version, &ctx);
    }

    pub fn apply_seed(&mut self, seed: SeedHeader) {
        let ctx = self.merge_context(None);
        let version = seed.version;
        self.seeds.insert(seed, version, &ctx);
    }

    pub fn update_seed_status(&mut self, status: SeedStatus, hlc: Hlc) {
        let ctx = self.merge_context(None);
        let next = Lww::new(status, hlc);
        self.seed_status.merge(&next, &ctx);
    }

    pub fn update_model_topk(&mut self, entries: Vec<String>, hlc: Hlc) {
        let ctx = self.merge_context(None);
        let next = Lww::new(ResonantModelTopK::new(entries, hlc), hlc);
        self.model_topk.merge(&next, &ctx);
    }

    pub fn update_policy(&mut self, policy: PolicyBundle) {
        let ctx = self.merge_context(None);
        let version = policy.version;
        let next = Lww::new(policy, version);
        self.policies.merge(&next, &ctx);
    }

    pub fn summary(&self) -> Summary {
        let ctx = self.merge_context(None);
        let models = self.models.elements(&ctx);
        let epochs = self.epochs.elements(&ctx);
        let seeds = self.seeds.elements(&ctx);

        let mut hasher = Hasher::new();
        hasher.update(&self.peer.to_bytes());
        hasher.update(&self.model_topk.clock.phys_ms.to_be_bytes());
        hasher.update(&self.model_topk.clock.log.to_be_bytes());
        for model in &models {
            hasher.update(model.id.as_bytes());
            hasher.update(model.hash.as_bytes());
        }
        for epoch in &epochs {
            hasher.update(&epoch.id.to_be_bytes());
            hasher.update(epoch.hash.as_bytes());
        }
        for seed in &seeds {
            hasher.update(seed.id.as_bytes());
            hasher.update(&seed.epoch.to_be_bytes());
        }
        hasher.update(self.seed_status.value.as_str().as_bytes());
        hasher.update(&self.seed_status.clock.phys_ms.to_be_bytes());
        hasher.update(&self.seed_status.clock.log.to_be_bytes());
        hasher.update(self.policies.value.hash.as_bytes());
        hasher.update(&self.policies.clock.phys_ms.to_be_bytes());
        hasher.update(&self.policies.clock.log.to_be_bytes());

        let digest = hasher.finalize().to_hex().to_string();
        let credence_sum: f32 = self.credence.values().copied().sum();

        Summary {
            peer: self.peer,
            digest,
            model_count: models.len(),
            epoch_count: epochs.len(),
            seed_count: seeds.len(),
            topk_clock: self.model_topk.clock,
            policy_clock: self.policies.clock,
            credence_sum,
        }
    }

    fn merge_context(&self, other: Option<&NoeticState>) -> MergeContext {
        let mut ctx = MergeContext::default();
        for (peer, value) in &self.credence {
            ctx.set_peer(*peer, *value);
        }
        if let Some(other) = other {
            for (peer, value) in &other.credence {
                let current = ctx.credence_for(peer);
                if *value > current {
                    ctx.set_peer(*peer, *value);
                }
            }
        }
        ctx
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub peer: PeerId,
    pub digest: String,
    pub model_count: usize,
    pub epoch_count: usize,
    pub seed_count: usize,
    pub topk_clock: Hlc,
    pub policy_clock: Hlc,
    pub credence_sum: f32,
}

impl Summary {
    pub fn is_in_sync_with(&self, other: &Summary) -> bool {
        self.digest == other.digest
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoeticSnapshot {
    pub peer: PeerId,
    pub credence: BTreeMap<PeerId, f32>,
    pub models: Vec<ResonantModelHeader>,
    pub epochs: Vec<EpochHeader>,
    pub seeds: Vec<SeedHeader>,
    pub model_topk: ResonantModelTopK,
    pub seed_status: SeedStatus,
    pub seed_status_clock: Hlc,
    pub policies: PolicyBundle,
}

impl From<&NoeticState> for NoeticSnapshot {
    fn from(state: &NoeticState) -> Self {
        let ctx = state.merge_context(None);
        NoeticSnapshot {
            peer: state.peer,
            credence: state.credence.clone(),
            models: state.models.elements(&ctx),
            epochs: state.epochs.elements(&ctx),
            seeds: state.seeds.elements(&ctx),
            model_topk: state.model_topk.value.clone(),
            seed_status: state.seed_status.value.clone(),
            seed_status_clock: state.seed_status.clock,
            policies: state.policies.value.clone(),
        }
    }
}

impl NoeticState {
    pub fn restore(snapshot: NoeticSnapshot) -> Self {
        let NoeticSnapshot {
            peer,
            credence,
            models,
            epochs,
            seeds,
            model_topk,
            seed_status,
            seed_status_clock,
            policies,
        } = snapshot;

        let mut state = NoeticState::new(peer);
        state.credence = credence;
        let ctx = state.merge_context(None);
        for model in models {
            let version = model.version;
            state.models.insert(model, version, &ctx);
        }
        for epoch in epochs {
            let version = epoch.version;
            state.epochs.insert(epoch, version, &ctx);
        }
        for seed in seeds {
            let version = seed.version;
            state.seeds.insert(seed, version, &ctx);
        }
        let topk_clock = model_topk.updated;
        state.model_topk = Lww::new(model_topk, topk_clock);
        state.seed_status = Lww::new(seed_status, seed_status_clock);
        let policy_bundle = policies;
        let policy_clock = policy_bundle.version;
        state.policies = Lww::new(policy_bundle, policy_clock);
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::PeerId;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn summary_digest_changes_with_models() {
        let signer = SigningKey::generate(&mut OsRng);
        let peer = PeerId::from_keypair(&signer);
        let mut state = NoeticState::new(peer);
        let header = ResonantModelHeader {
            id: "model/a".into(),
            version: Hlc::new(peer, 10),
            hash: "deadbeef".into(),
            source: peer,
        };
        state.apply_model_header(header);
        let summary_a = state.summary();
        state.update_seed_status(SeedStatus::Active { started_ms: 20 }, Hlc::new(peer, 20));
        let summary_b = state.summary();
        assert_ne!(summary_a.digest, summary_b.digest);
    }
}
