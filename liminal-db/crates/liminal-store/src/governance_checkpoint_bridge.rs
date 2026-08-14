use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    sha256_ref, verify_signed_checkpoint, CheckpointLedgerExt, CheckpointSigner,
    SignedCheckpointManifest, TransitionEventInput, TransitionLinks, TransitionRecordKind,
    TrustedCheckpointKey, TrustedKeyRegistry, TrustworthyTransitionLedger,
};

pub const GOVERNANCE_ENVELOPE_SCHEMA: &str = "liminalosai-governance-transition-envelope-v0.1";
pub const GOVERNANCE_RECEIPT_SCHEMA: &str =
    "liminaldb-liminalosai-governance-checkpoint-receipt-v0.1";

const SUBJECT_ID: &str = "liminalosai:durable-governance";
const VERIFIED_STATUS: &str = "LOCAL_SIGNATURE_VERIFIED";
const ZERO_SHA256: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceTransitionEnvelopeBody {
    pub schema: String,
    pub root_id_sha256: String,
    pub transition_kind: String,
    pub generation_before: u64,
    pub generation_after: u64,
    pub world_before_sha256: String,
    pub world_after_sha256: String,
    pub reservation_sha256: String,
    pub operation_sha256: String,
    pub upstream_receipt_sha256: String,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceTransitionEnvelope {
    pub body: GovernanceTransitionEnvelopeBody,
    pub envelope_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceCheckpointReceiptBody {
    pub schema: String,
    pub envelope_ref: String,
    pub root_id_sha256: String,
    pub transition_kind: String,
    pub generation_before: u64,
    pub generation_after: u64,
    pub world_before_sha256: String,
    pub world_after_sha256: String,
    pub reservation_sha256: String,
    pub operation_sha256: String,
    pub upstream_receipt_sha256: String,
    pub event_hash: String,
    pub checkpoint_ref: String,
    pub event_chain_head: String,
    pub last_sequence: u64,
    pub projection_digest: String,
    pub snapshot_digest: String,
    pub signer_id: String,
    pub key_id: String,
    pub verification_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceCheckpointReceipt {
    pub body: GovernanceCheckpointReceiptBody,
    pub receipt_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceCheckpointBundle {
    pub envelope: GovernanceTransitionEnvelope,
    pub receipt: GovernanceCheckpointReceipt,
    pub checkpoint: SignedCheckpointManifest,
    pub trusted_key: TrustedCheckpointKey,
}

#[derive(Debug, Error)]
pub enum GovernanceCheckpointBridgeError {
    #[error("invalid governance field: {0}")]
    InvalidField(&'static str),
    #[error("governance envelope digest mismatch")]
    EnvelopeDigestMismatch,
    #[error("transition ledger error: {0}")]
    Ledger(String),
    #[error("checkpoint error: {0}")]
    Checkpoint(String),
    #[error("encoding error: {0}")]
    Encoding(String),
}

impl GovernanceTransitionEnvelope {
    pub fn build(
        body: GovernanceTransitionEnvelopeBody,
    ) -> Result<Self, GovernanceCheckpointBridgeError> {
        validate_body(&body)?;
        let envelope_ref = digest_cbor(&body)?;
        Ok(Self { body, envelope_ref })
    }

    pub fn verify(&self) -> Result<(), GovernanceCheckpointBridgeError> {
        validate_body(&self.body)?;
        if digest_cbor(&self.body)? != self.envelope_ref {
            return Err(GovernanceCheckpointBridgeError::EnvelopeDigestMismatch);
        }
        Ok(())
    }
}

impl GovernanceCheckpointReceipt {
    pub fn verify(&self) -> Result<(), GovernanceCheckpointBridgeError> {
        if self.body.schema != GOVERNANCE_RECEIPT_SCHEMA
            || self.body.verification_status != VERIFIED_STATUS
        {
            return Err(GovernanceCheckpointBridgeError::InvalidField(
                "receipt_schema_or_status",
            ));
        }
        validate_ref(&self.body.envelope_ref, "envelope_ref")?;
        validate_ref(&self.body.event_hash, "event_hash")?;
        validate_ref(&self.body.checkpoint_ref, "checkpoint_ref")?;
        validate_ref(&self.body.event_chain_head, "event_chain_head")?;
        validate_ref(&self.body.projection_digest, "projection_digest")?;
        validate_ref(&self.body.snapshot_digest, "snapshot_digest")?;
        validate_raw_sha(&self.body.root_id_sha256, "root_id_sha256", false)?;
        validate_raw_sha(&self.body.world_before_sha256, "world_before_sha256", true)?;
        validate_raw_sha(&self.body.world_after_sha256, "world_after_sha256", false)?;
        validate_raw_sha(&self.body.reservation_sha256, "reservation_sha256", true)?;
        validate_raw_sha(&self.body.operation_sha256, "operation_sha256", true)?;
        validate_raw_sha(
            &self.body.upstream_receipt_sha256,
            "upstream_receipt_sha256",
            false,
        )?;
        if digest_cbor(&self.body)? != self.receipt_ref {
            return Err(GovernanceCheckpointBridgeError::InvalidField("receipt_ref"));
        }
        Ok(())
    }
}

pub fn append_governance_checkpoint<P: AsRef<Path>>(
    root: P,
    envelope: GovernanceTransitionEnvelope,
    signer: &CheckpointSigner,
    issued_at_ms: u64,
) -> Result<GovernanceCheckpointBundle, GovernanceCheckpointBridgeError> {
    envelope.verify()?;
    if issued_at_ms < envelope.body.captured_at_ms {
        return Err(GovernanceCheckpointBridgeError::InvalidField(
            "issued_at_ms",
        ));
    }

    let mut ledger = TrustworthyTransitionLedger::open(root.as_ref())
        .map_err(|error| GovernanceCheckpointBridgeError::Ledger(error.to_string()))?;
    let transition_id = format!(
        "liminalosai-governance-{}",
        envelope.envelope_ref.trim_start_matches("sha256:")
    );
    let event = ledger
        .append(TransitionEventInput {
            transition_id,
            subject_id: SUBJECT_ID.to_owned(),
            kind: TransitionRecordKind::Authorization,
            record_ref: envelope.envelope_ref.clone(),
            payload_digest: envelope.envelope_ref.clone(),
            links: TransitionLinks::default(),
            dimensions: None,
            side_effect_committed: None,
            captured_at_ms: envelope.body.captured_at_ms,
        })
        .map_err(|error| GovernanceCheckpointBridgeError::Ledger(error.to_string()))?;

    let snapshot = ledger
        .write_snapshot(issued_at_ms)
        .map_err(|error| GovernanceCheckpointBridgeError::Ledger(error.to_string()))?;
    let storage_root_identity = format!("sha256:{}", envelope.body.root_id_sha256);
    let material = ledger
        .checkpoint_material(storage_root_identity, &snapshot)
        .map_err(|error| GovernanceCheckpointBridgeError::Checkpoint(error.to_string()))?;
    let checkpoint = signer
        .sign(material, issued_at_ms, None, None)
        .map_err(|error| GovernanceCheckpointBridgeError::Checkpoint(error.to_string()))?;
    let trusted_key = signer.trusted_key(0, None, None);
    let registry = TrustedKeyRegistry::default()
        .with_key(trusted_key.clone())
        .map_err(|error| GovernanceCheckpointBridgeError::Checkpoint(error.to_string()))?;
    verify_signed_checkpoint(&checkpoint, &registry, issued_at_ms)
        .map_err(|error| GovernanceCheckpointBridgeError::Checkpoint(error.to_string()))?;

    let receipt_body = GovernanceCheckpointReceiptBody {
        schema: GOVERNANCE_RECEIPT_SCHEMA.to_owned(),
        envelope_ref: envelope.envelope_ref.clone(),
        root_id_sha256: envelope.body.root_id_sha256.clone(),
        transition_kind: envelope.body.transition_kind.clone(),
        generation_before: envelope.body.generation_before,
        generation_after: envelope.body.generation_after,
        world_before_sha256: envelope.body.world_before_sha256.clone(),
        world_after_sha256: envelope.body.world_after_sha256.clone(),
        reservation_sha256: envelope.body.reservation_sha256.clone(),
        operation_sha256: envelope.body.operation_sha256.clone(),
        upstream_receipt_sha256: envelope.body.upstream_receipt_sha256.clone(),
        event_hash: event.event_hash.clone(),
        checkpoint_ref: checkpoint.manifest_ref.clone(),
        event_chain_head: checkpoint.body.event_chain_head.clone(),
        last_sequence: checkpoint.body.last_sequence,
        projection_digest: checkpoint.body.projection_digest.clone(),
        snapshot_digest: checkpoint.body.snapshot_digest.clone(),
        signer_id: checkpoint.body.signer_id.clone(),
        key_id: checkpoint.body.key_id.clone(),
        verification_status: VERIFIED_STATUS.to_owned(),
    };
    let receipt = GovernanceCheckpointReceipt {
        receipt_ref: digest_cbor(&receipt_body)?,
        body: receipt_body,
    };
    receipt.verify()?;

    Ok(GovernanceCheckpointBundle {
        envelope,
        receipt,
        checkpoint,
        trusted_key,
    })
}

fn validate_body(
    body: &GovernanceTransitionEnvelopeBody,
) -> Result<(), GovernanceCheckpointBridgeError> {
    if body.schema != GOVERNANCE_ENVELOPE_SCHEMA {
        return Err(GovernanceCheckpointBridgeError::InvalidField("schema"));
    }
    validate_raw_sha(&body.root_id_sha256, "root_id_sha256", false)?;
    validate_raw_sha(&body.world_before_sha256, "world_before_sha256", true)?;
    validate_raw_sha(&body.world_after_sha256, "world_after_sha256", false)?;
    validate_raw_sha(&body.reservation_sha256, "reservation_sha256", true)?;
    validate_raw_sha(&body.operation_sha256, "operation_sha256", true)?;
    validate_raw_sha(
        &body.upstream_receipt_sha256,
        "upstream_receipt_sha256",
        false,
    )?;
    match body.transition_kind.as_str() {
        "initialize" => {
            if body.generation_before != 0
                || body.generation_after != 0
                || body.world_before_sha256 != ZERO_SHA256
                || body.reservation_sha256 != ZERO_SHA256
                || body.operation_sha256 != ZERO_SHA256
            {
                return Err(GovernanceCheckpointBridgeError::InvalidField(
                    "initialize_semantics",
                ));
            }
        }
        "reserve" => {
            if body.generation_after != body.generation_before
                || body.world_after_sha256 != body.world_before_sha256
                || body.reservation_sha256 == ZERO_SHA256
                || body.operation_sha256 == ZERO_SHA256
            {
                return Err(GovernanceCheckpointBridgeError::InvalidField(
                    "reserve_semantics",
                ));
            }
        }
        "commit" => {
            if body.generation_after != body.generation_before.saturating_add(1)
                || body.reservation_sha256 == ZERO_SHA256
                || body.operation_sha256 == ZERO_SHA256
            {
                return Err(GovernanceCheckpointBridgeError::InvalidField(
                    "commit_semantics",
                ));
            }
        }
        "mutate" => {
            if body.generation_after != body.generation_before.saturating_add(1)
                || body.reservation_sha256 != ZERO_SHA256
                || body.operation_sha256 != ZERO_SHA256
            {
                return Err(GovernanceCheckpointBridgeError::InvalidField(
                    "mutate_semantics",
                ));
            }
        }
        "reconcile" => {
            if body.generation_after != body.generation_before.saturating_add(1)
                || body.reservation_sha256 == ZERO_SHA256
                || body.operation_sha256 != ZERO_SHA256
            {
                return Err(GovernanceCheckpointBridgeError::InvalidField(
                    "reconcile_semantics",
                ));
            }
        }
        _ => {
            return Err(GovernanceCheckpointBridgeError::InvalidField(
                "transition_kind",
            ))
        }
    }
    Ok(())
}

fn validate_raw_sha(
    value: &str,
    label: &'static str,
    allow_zero: bool,
) -> Result<(), GovernanceCheckpointBridgeError> {
    let valid = value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid || (!allow_zero && value == ZERO_SHA256) {
        return Err(GovernanceCheckpointBridgeError::InvalidField(label));
    }
    Ok(())
}

fn validate_ref(value: &str, label: &'static str) -> Result<(), GovernanceCheckpointBridgeError> {
    let valid = value.len() == 71
        && value.starts_with("sha256:")
        && value.as_bytes()[7..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(GovernanceCheckpointBridgeError::InvalidField(label));
    }
    Ok(())
}

fn digest_cbor<T: Serialize>(value: &T) -> Result<String, GovernanceCheckpointBridgeError> {
    let bytes = serde_cbor::to_vec(value)
        .map_err(|error| GovernanceCheckpointBridgeError::Encoding(error.to_string()))?;
    Ok(sha256_ref(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(kind: &str, before: u64, after: u64) -> GovernanceTransitionEnvelopeBody {
        GovernanceTransitionEnvelopeBody {
            schema: GOVERNANCE_ENVELOPE_SCHEMA.to_owned(),
            root_id_sha256: "1".repeat(64),
            transition_kind: kind.to_owned(),
            generation_before: before,
            generation_after: after,
            world_before_sha256: if kind == "initialize" {
                ZERO_SHA256.to_owned()
            } else {
                "2".repeat(64)
            },
            world_after_sha256: "3".repeat(64),
            reservation_sha256: if matches!(kind, "reserve" | "commit" | "reconcile") {
                "4".repeat(64)
            } else {
                ZERO_SHA256.to_owned()
            },
            operation_sha256: if matches!(kind, "reserve" | "commit") {
                "5".repeat(64)
            } else {
                ZERO_SHA256.to_owned()
            },
            upstream_receipt_sha256: "6".repeat(64),
            captured_at_ms: 10,
        }
    }

    #[test]
    fn restart_recovery_and_signed_checkpoint_advance() {
        let dir = tempfile::tempdir().unwrap();
        let signer = CheckpointSigner::from_seed(
            "liminalosai-governance-bridge",
            "test-key-v0.1",
            [7u8; 32],
        )
        .unwrap();

        let mut reserve = body("reserve", 0, 0);
        reserve.world_after_sha256 = reserve.world_before_sha256.clone();
        let first = append_governance_checkpoint(
            dir.path(),
            GovernanceTransitionEnvelope::build(reserve).unwrap(),
            &signer,
            11,
        )
        .unwrap();
        assert_eq!(first.receipt.body.last_sequence, 1);
        first.receipt.verify().unwrap();

        let commit = append_governance_checkpoint(
            dir.path(),
            GovernanceTransitionEnvelope::build(body("commit", 0, 1)).unwrap(),
            &signer,
            12,
        )
        .unwrap();
        assert_eq!(commit.receipt.body.last_sequence, 2);
        assert_ne!(
            first.receipt.body.event_chain_head,
            commit.receipt.body.event_chain_head
        );
        let registry = TrustedKeyRegistry::default()
            .with_key(commit.trusted_key.clone())
            .unwrap();
        verify_signed_checkpoint(&commit.checkpoint, &registry, 12).unwrap();
    }

    #[test]
    fn rejects_invalid_generation_semantics() {
        let invalid = body("commit", 3, 3);
        assert!(GovernanceTransitionEnvelope::build(invalid).is_err());
    }

    #[test]
    fn receipt_detects_tamper() {
        let dir = tempfile::tempdir().unwrap();
        let signer = CheckpointSigner::from_seed("bridge", "key", [9u8; 32]).unwrap();
        let mut reserve = body("reserve", 0, 0);
        reserve.world_after_sha256 = reserve.world_before_sha256.clone();
        let mut bundle = append_governance_checkpoint(
            dir.path(),
            GovernanceTransitionEnvelope::build(reserve).unwrap(),
            &signer,
            11,
        )
        .unwrap();
        bundle.receipt.body.world_after_sha256 = "f".repeat(64);
        assert!(bundle.receipt.verify().is_err());
    }
}
