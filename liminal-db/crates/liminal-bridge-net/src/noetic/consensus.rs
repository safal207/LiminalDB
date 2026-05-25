use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::crdt::Hlc;
use super::state::{PolicyBundle, ResonantModelHeader, ResonantModelTopK, SeedHeader, SeedStatus};
use crate::peers::PeerId;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProposalId(Uuid);

impl ProposalId {
    pub fn new() -> Self {
        ProposalId(Uuid::new_v4())
    }
}

impl fmt::Display for ProposalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ProposalObject {
    Model {
        header: ResonantModelHeader,
        topk: ResonantModelTopK,
    },
    Seed {
        header: SeedHeader,
        status: SeedStatus,
    },
    Policy {
        bundle: PolicyBundle,
    },
}

impl ProposalObject {
    fn kind(&self) -> ProposalKind {
        match self {
            ProposalObject::Model { .. } => ProposalKind::Model,
            ProposalObject::Seed { .. } => ProposalKind::Seed,
            ProposalObject::Policy { .. } => ProposalKind::Policy,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VoteDecision {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoteRecord {
    pub peer: PeerId,
    pub decision: VoteDecision,
    pub credence: f32,
    pub clock: Hlc,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proposal {
    pub id: ProposalId,
    pub proposer: PeerId,
    pub object: ProposalObject,
    pub clock: Hlc,
    pub votes: Vec<VoteRecord>,
    pub committed: bool,
    pub credence_sum: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsensusEngine {
    quorum: f32,
    proposals: BTreeMap<ProposalId, Proposal>,
}

impl ConsensusEngine {
    pub fn new(quorum: f32) -> Self {
        ConsensusEngine {
            quorum,
            proposals: BTreeMap::new(),
        }
    }

    pub fn quorum(&self) -> f32 {
        self.quorum
    }

    pub fn set_quorum(&mut self, quorum: f32) {
        self.quorum = quorum.max(0.0);
    }

    pub fn proposals(&self) -> &BTreeMap<ProposalId, Proposal> {
        &self.proposals
    }

    pub fn propose_model(
        &mut self,
        proposer: PeerId,
        header: ResonantModelHeader,
        topk: ResonantModelTopK,
        clock: Hlc,
    ) -> NoeticEvent {
        let id = ProposalId::new();
        let proposal = Proposal {
            id,
            proposer,
            object: ProposalObject::Model { header, topk },
            clock,
            votes: Vec::new(),
            committed: false,
            credence_sum: 0.0,
        };
        let event = Self::event_for(self.quorum, &proposal, Phase::Propose);
        self.proposals.insert(id, proposal);
        event
    }

    pub fn propose_seed(
        &mut self,
        proposer: PeerId,
        header: SeedHeader,
        status: SeedStatus,
        clock: Hlc,
    ) -> NoeticEvent {
        let id = ProposalId::new();
        let proposal = Proposal {
            id,
            proposer,
            object: ProposalObject::Seed { header, status },
            clock,
            votes: Vec::new(),
            committed: false,
            credence_sum: 0.0,
        };
        let event = Self::event_for(self.quorum, &proposal, Phase::Propose);
        self.proposals.insert(id, proposal);
        event
    }

    pub fn propose_policy(
        &mut self,
        proposer: PeerId,
        bundle: PolicyBundle,
        clock: Hlc,
    ) -> NoeticEvent {
        let id = ProposalId::new();
        let proposal = Proposal {
            id,
            proposer,
            object: ProposalObject::Policy { bundle },
            clock,
            votes: Vec::new(),
            committed: false,
            credence_sum: 0.0,
        };
        let event = Self::event_for(self.quorum, &proposal, Phase::Propose);
        self.proposals.insert(id, proposal);
        event
    }

    pub fn vote(
        &mut self,
        proposal_id: ProposalId,
        peer: PeerId,
        decision: VoteDecision,
        credence: f32,
        clock: Hlc,
    ) -> Option<NoeticEvent> {
        let proposal = self.proposals.get_mut(&proposal_id)?;
        let mut updated = false;
        if let Some(record) = proposal.votes.iter_mut().find(|vote| vote.peer == peer) {
            if is_newer(&clock, &record.clock) {
                if record.decision == VoteDecision::Approve {
                    proposal.credence_sum -= record.credence;
                }
                record.decision = decision;
                record.credence = credence;
                record.clock = clock;
                if decision == VoteDecision::Approve {
                    proposal.credence_sum += credence;
                }
                updated = true;
            }
        } else {
            let record = VoteRecord {
                peer,
                decision,
                credence,
                clock,
            };
            if decision == VoteDecision::Approve {
                proposal.credence_sum += credence;
            }
            proposal.votes.push(record);
            updated = true;
        }

        if !updated {
            return None;
        }

        if !proposal.committed && proposal.credence_sum >= self.quorum {
            if proposal
                .votes
                .iter()
                .all(|vote| vote.decision == VoteDecision::Approve)
            {
                proposal.committed = true;
                let commit_event = Self::event_for(self.quorum, proposal, Phase::Commit);
                return Some(commit_event);
            }
        }
        Some(Self::event_for(self.quorum, proposal, Phase::Vote))
    }

    pub fn commit(&mut self, proposal_id: ProposalId) -> Option<NoeticEvent> {
        let proposal = self.proposals.get_mut(&proposal_id)?;
        if proposal.committed {
            return None;
        }
        proposal.committed = true;
        Some(Self::event_for(self.quorum, proposal, Phase::Commit))
    }

    fn event_for(quorum: f32, proposal: &Proposal, phase: Phase) -> NoeticEvent {
        NoeticEvent {
            ev: "noetic",
            meta: EventMeta {
                phase,
                obj: proposal.object.kind(),
                id: proposal.id,
                credence_sum: proposal.credence_sum,
                quorum,
            },
        }
    }

    pub fn snapshot(&self) -> ConsensusSnapshot {
        let proposals = self.proposals.values().cloned().collect();
        ConsensusSnapshot {
            quorum: self.quorum,
            proposals,
        }
    }

    pub fn restore(snapshot: ConsensusSnapshot) -> Self {
        let mut proposals = BTreeMap::new();
        for proposal in snapshot.proposals {
            proposals.insert(proposal.id, proposal);
        }
        ConsensusEngine {
            quorum: snapshot.quorum,
            proposals,
        }
    }
}

fn is_newer(a: &Hlc, b: &Hlc) -> bool {
    if a.phys_ms > b.phys_ms {
        true
    } else if a.phys_ms < b.phys_ms {
        false
    } else if a.log > b.log {
        true
    } else if a.log < b.log {
        false
    } else {
        a.node > b.node
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Propose,
    Vote,
    Commit,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProposalKind {
    Model,
    Seed,
    Policy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventMeta {
    pub phase: Phase,
    pub obj: ProposalKind,
    pub id: ProposalId,
    pub credence_sum: f32,
    pub quorum: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoeticEvent {
    pub ev: &'static str,
    pub meta: EventMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsensusSnapshot {
    pub quorum: f32,
    pub proposals: Vec<Proposal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    #[test]
    fn proposal_reaches_quorum() {
        let signer_a = SigningKey::generate(&mut OsRng);
        let signer_b = SigningKey::generate(&mut OsRng);
        let peer_a = PeerId::from_keypair(&signer_a);
        let peer_b = PeerId::from_keypair(&signer_b);
        let mut engine = ConsensusEngine::new(1.1);
        let version = Hlc::new(peer_a, 10);
        let header = ResonantModelHeader {
            id: "model/a".into(),
            version,
            hash: "deadbeef".into(),
            source: peer_a,
        };
        let topk = ResonantModelTopK::new(vec!["model/a".into()], version);
        let event = engine.propose_model(peer_a, header, topk, version);
        assert_eq!(event.meta.phase, Phase::Propose);

        let proposal_id = event.meta.id;
        let vote_event = engine
            .vote(
                proposal_id,
                peer_a,
                VoteDecision::Approve,
                0.6,
                Hlc::new(peer_a, 11),
            )
            .expect("vote should generate event");
        assert_eq!(vote_event.meta.phase, Phase::Vote);

        let commit_event = engine
            .vote(
                proposal_id,
                peer_b,
                VoteDecision::Approve,
                0.6,
                Hlc::new(peer_b, 12),
            )
            .expect("commit event");
        assert_eq!(commit_event.meta.phase, Phase::Commit);
        assert!(engine.proposals.get(&proposal_id).unwrap().committed);
    }

    #[test]
    fn rejected_vote_prevents_commit() {
        let signer_a = SigningKey::generate(&mut OsRng);
        let signer_b = SigningKey::generate(&mut OsRng);
        let peer_a = PeerId::from_keypair(&signer_a);
        let peer_b = PeerId::from_keypair(&signer_b);
        let mut engine = ConsensusEngine::new(0.8);
        let version = Hlc::new(peer_a, 10);
        let header = ResonantModelHeader {
            id: "model/a".into(),
            version,
            hash: "feedface".into(),
            source: peer_a,
        };
        let topk = ResonantModelTopK::new(vec!["model/a".into()], version);
        let event = engine.propose_model(peer_a, header, topk, version);
        let proposal_id = event.meta.id;

        engine
            .vote(
                proposal_id,
                peer_a,
                VoteDecision::Approve,
                0.5,
                Hlc::new(peer_a, 11),
            )
            .expect("first vote");

        let vote = engine
            .vote(
                proposal_id,
                peer_b,
                VoteDecision::Reject,
                0.6,
                Hlc::new(peer_b, 12),
            )
            .expect("second vote");
        assert_eq!(vote.meta.phase, Phase::Vote);
        assert!(!engine.proposals.get(&proposal_id).unwrap().committed);
    }
}
