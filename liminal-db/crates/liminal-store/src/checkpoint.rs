use std::collections::BTreeMap;
use std::convert::TryFrom;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::sha256_ref;
use crate::trustworthy_transition::{TransitionLedgerSnapshotInfo, TrustworthyTransitionLedger};

const CHECKPOINT_SCHEMA: &str = "liminaldb.signed-checkpoint-manifest.v0.1";
const CHECKPOINT_PROFILE: &str = "org.liminaldb.signed-checkpoint.v0.1";
const LEDGER_PROFILE: &str = "org.liminaldb.trustworthy-transition-ledger.v0.1";
const SIGNATURE_ALGORITHM: &str = "Ed25519";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointMaterial {
    pub ledger_profile: String,
    pub storage_root_identity: String,
    pub event_chain_head: String,
    pub last_sequence: u64,
    pub wal_segment: u64,
    pub wal_offset: u64,
    pub projection_digest: String,
    pub snapshot_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointManifestBody {
    pub schema: String,
    pub checkpoint_profile: String,
    pub ledger_profile: String,
    pub storage_root_identity: String,
    pub event_chain_head: String,
    pub last_sequence: u64,
    pub wal_segment: u64,
    pub wal_offset: u64,
    pub projection_digest: String,
    pub snapshot_digest: String,
    pub signer_id: String,
    pub key_id: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: Option<u64>,
    pub previous_checkpoint_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedCheckpointManifest {
    pub body: CheckpointManifestBody,
    pub manifest_ref: String,
    pub signature_algorithm: String,
    pub signature_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedCheckpointKey {
    pub signer_id: String,
    pub key_id: String,
    pub public_key_hex: String,
    pub valid_from_ms: u64,
    pub valid_until_ms: Option<u64>,
    pub revoked_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct TrustedKeyRegistry {
    keys: BTreeMap<String, TrustedCheckpointKey>,
}

#[derive(Clone)]
pub struct CheckpointSigner {
    signer_id: String,
    key_id: String,
    signing_key: SigningKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalAnchor {
    pub provider_profile: String,
    pub anchor_id: String,
    pub checkpoint_ref: String,
    pub storage_root_identity: String,
    pub event_chain_head: String,
    pub last_sequence: u64,
    pub anchored_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AntiRollbackStatus {
    LocalSignatureOnly,
    ExternalAnchorVerified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifiedCheckpointChain {
    pub status: AntiRollbackStatus,
    pub storage_root_identity: String,
    pub first_checkpoint_ref: String,
    pub latest_checkpoint_ref: String,
    pub latest_event_chain_head: String,
    pub latest_sequence: u64,
    pub checkpoint_count: usize,
    pub external_anchor_id: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CheckpointError {
    #[error("checkpoint encoding error: {0}")]
    Encoding(String),
    #[error("invalid checkpoint field: {0}")]
    InvalidField(&'static str),
    #[error("invalid sha256 reference: {0}")]
    InvalidReference(&'static str),
    #[error("invalid hex value: {0}")]
    InvalidHex(&'static str),
    #[error("unsupported signature algorithm")]
    UnsupportedSignatureAlgorithm,
    #[error("checkpoint manifest reference mismatch")]
    ManifestReferenceMismatch,
    #[error("checkpoint signature verification failed")]
    SignatureVerificationFailed,
    #[error("trusted checkpoint key is unknown")]
    UnknownKey,
    #[error("checkpoint key is not valid at trusted verification time")]
    KeyNotYetValid,
    #[error("checkpoint key is expired at trusted verification time")]
    KeyExpired,
    #[error("checkpoint key is revoked at trusted verification time")]
    KeyRevoked,
    #[error("checkpoint issuance time is later than trusted verification time")]
    CheckpointFromFuture,
    #[error("checkpoint was expired at verification time")]
    CheckpointExpired,
    #[error("checkpoint expiry must be later than issuance")]
    InvalidExpiry,
    #[error("checkpoint chain is empty")]
    EmptyChain,
    #[error("checkpoint chain changed ledger or signer identity")]
    ChainIdentityMismatch,
    #[error("checkpoint previous reference does not match")]
    PreviousCheckpointMismatch,
    #[error("checkpoint sequence or WAL position is not monotonic")]
    NonMonotonicCheckpoint,
    #[error("checkpoint sequence advanced without a new ledger head")]
    LedgerHeadNotAdvanced,
    #[error("checkpoint is older than the latest trusted external anchor")]
    ExternalAnchorRollback,
    #[error("checkpoint conflicts with the trusted external anchor")]
    ExternalAnchorFork,
    #[error("trusted external anchor is not present in the supplied chain")]
    ExternalAnchorNotInChain,
    #[error("ledger has no event-chain head")]
    MissingLedgerHead,
    #[error("snapshot metadata does not match the current ledger state")]
    SnapshotStateMismatch,
}

impl CheckpointError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Encoding(_) => "ENCODING_ERROR",
            Self::InvalidField(_) => "INVALID_FIELD",
            Self::InvalidReference(_) => "INVALID_REFERENCE",
            Self::InvalidHex(_) => "INVALID_HEX",
            Self::UnsupportedSignatureAlgorithm => "UNSUPPORTED_SIGNATURE_ALGORITHM",
            Self::ManifestReferenceMismatch => "MANIFEST_REFERENCE_MISMATCH",
            Self::SignatureVerificationFailed => "SIGNATURE_VERIFICATION_FAILED",
            Self::UnknownKey => "UNKNOWN_KEY",
            Self::KeyNotYetValid => "KEY_NOT_YET_VALID",
            Self::KeyExpired => "KEY_EXPIRED",
            Self::KeyRevoked => "KEY_REVOKED",
            Self::CheckpointFromFuture => "CHECKPOINT_FROM_FUTURE",
            Self::CheckpointExpired => "CHECKPOINT_EXPIRED",
            Self::InvalidExpiry => "INVALID_EXPIRY",
            Self::EmptyChain => "EMPTY_CHAIN",
            Self::ChainIdentityMismatch => "CHAIN_IDENTITY_MISMATCH",
            Self::PreviousCheckpointMismatch => "PREVIOUS_CHECKPOINT_MISMATCH",
            Self::NonMonotonicCheckpoint => "NON_MONOTONIC_CHECKPOINT",
            Self::LedgerHeadNotAdvanced => "LEDGER_HEAD_NOT_ADVANCED",
            Self::ExternalAnchorRollback => "EXTERNAL_ANCHOR_ROLLBACK",
            Self::ExternalAnchorFork => "EXTERNAL_ANCHOR_FORK",
            Self::ExternalAnchorNotInChain => "EXTERNAL_ANCHOR_NOT_IN_CHAIN",
            Self::MissingLedgerHead => "MISSING_LEDGER_HEAD",
            Self::SnapshotStateMismatch => "SNAPSHOT_STATE_MISMATCH",
        }
    }
}

pub trait CheckpointLedgerExt {
    fn checkpoint_material(
        &self,
        storage_root_identity: String,
        snapshot: &TransitionLedgerSnapshotInfo,
    ) -> Result<CheckpointMaterial, CheckpointError>;
}

impl CheckpointLedgerExt for TrustworthyTransitionLedger {
    fn checkpoint_material(
        &self,
        storage_root_identity: String,
        snapshot: &TransitionLedgerSnapshotInfo,
    ) -> Result<CheckpointMaterial, CheckpointError> {
        validate_ref(&storage_root_identity, "storage_root_identity")?;
        validate_ref(&snapshot.snapshot_digest, "snapshot_digest")?;
        let event_chain_head = self
            .head_event_hash()
            .ok_or(CheckpointError::MissingLedgerHead)?
            .to_owned();
        validate_ref(&event_chain_head, "event_chain_head")?;
        let projection_digest = self
            .checkpoint_projection_digest()
            .map_err(|error| CheckpointError::Encoding(error.to_string()))?;
        if snapshot.path.as_path() != self.snapshot_path()
            || snapshot.event_count != self.event_count()
            || snapshot.projection_count != self.projections().len()
            || snapshot.head_event_hash.as_deref() != Some(event_chain_head.as_str())
            || snapshot.projection_digest != projection_digest
        {
            return Err(CheckpointError::SnapshotStateMismatch);
        }
        Ok(CheckpointMaterial {
            ledger_profile: LEDGER_PROFILE.to_owned(),
            storage_root_identity,
            event_chain_head,
            last_sequence: self.event_count(),
            wal_segment: snapshot.offset.segment,
            wal_offset: snapshot.offset.position,
            projection_digest,
            snapshot_digest: snapshot.snapshot_digest.clone(),
        })
    }
}

impl CheckpointSigner {
    pub fn from_seed(
        signer_id: impl Into<String>,
        key_id: impl Into<String>,
        seed: [u8; 32],
    ) -> Result<Self, CheckpointError> {
        let signer_id = normalized(signer_id.into(), "signer_id")?;
        let key_id = normalized(key_id.into(), "key_id")?;
        Ok(Self {
            signer_id,
            key_id,
            signing_key: SigningKey::from_bytes(&seed),
        })
    }

    pub fn from_seed_hex(
        signer_id: impl Into<String>,
        key_id: impl Into<String>,
        seed_hex: &str,
    ) -> Result<Self, CheckpointError> {
        let seed = decode_fixed::<32>(seed_hex, "seed_hex")?;
        Self::from_seed(signer_id, key_id, seed)
    }

    pub fn signer_id(&self) -> &str {
        &self.signer_id
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn public_key_hex(&self) -> String {
        encode_hex(&self.signing_key.verifying_key().to_bytes())
    }

    pub fn trusted_key(
        &self,
        valid_from_ms: u64,
        valid_until_ms: Option<u64>,
        revoked_at_ms: Option<u64>,
    ) -> TrustedCheckpointKey {
        TrustedCheckpointKey {
            signer_id: self.signer_id.clone(),
            key_id: self.key_id.clone(),
            public_key_hex: self.public_key_hex(),
            valid_from_ms,
            valid_until_ms,
            revoked_at_ms,
        }
    }

    pub fn sign(
        &self,
        material: CheckpointMaterial,
        issued_at_ms: u64,
        expires_at_ms: Option<u64>,
        previous_checkpoint_ref: Option<String>,
    ) -> Result<SignedCheckpointManifest, CheckpointError> {
        validate_material(&material)?;
        if let Some(expires_at_ms) = expires_at_ms {
            if expires_at_ms <= issued_at_ms {
                return Err(CheckpointError::InvalidExpiry);
            }
        }
        if let Some(reference) = &previous_checkpoint_ref {
            validate_ref(reference, "previous_checkpoint_ref")?;
        }
        let body = CheckpointManifestBody {
            schema: CHECKPOINT_SCHEMA.to_owned(),
            checkpoint_profile: CHECKPOINT_PROFILE.to_owned(),
            ledger_profile: material.ledger_profile,
            storage_root_identity: material.storage_root_identity,
            event_chain_head: material.event_chain_head,
            last_sequence: material.last_sequence,
            wal_segment: material.wal_segment,
            wal_offset: material.wal_offset,
            projection_digest: material.projection_digest,
            snapshot_digest: material.snapshot_digest,
            signer_id: self.signer_id.clone(),
            key_id: self.key_id.clone(),
            issued_at_ms,
            expires_at_ms,
            previous_checkpoint_ref,
        };
        let message = canonical_bytes(&body)?;
        let manifest_ref = sha256_ref(&message);
        let signature = self.signing_key.sign(&message);
        Ok(SignedCheckpointManifest {
            body,
            manifest_ref,
            signature_algorithm: SIGNATURE_ALGORITHM.to_owned(),
            signature_hex: encode_hex(&signature.to_bytes()),
        })
    }
}

impl TrustedKeyRegistry {
    pub fn insert(&mut self, key: TrustedCheckpointKey) -> Result<(), CheckpointError> {
        validate_trusted_key(&key)?;
        self.keys.insert(key_slot(&key.signer_id, &key.key_id), key);
        Ok(())
    }

    pub fn with_key(mut self, key: TrustedCheckpointKey) -> Result<Self, CheckpointError> {
        self.insert(key)?;
        Ok(self)
    }

    fn get(&self, signer_id: &str, key_id: &str) -> Option<&TrustedCheckpointKey> {
        self.keys.get(&key_slot(signer_id, key_id))
    }
}

pub fn verify_signed_checkpoint(
    manifest: &SignedCheckpointManifest,
    registry: &TrustedKeyRegistry,
    now_ms: u64,
) -> Result<(), CheckpointError> {
    validate_manifest_body(&manifest.body)?;
    if manifest.signature_algorithm != SIGNATURE_ALGORITHM {
        return Err(CheckpointError::UnsupportedSignatureAlgorithm);
    }
    let message = canonical_bytes(&manifest.body)?;
    if manifest.manifest_ref != sha256_ref(&message) {
        return Err(CheckpointError::ManifestReferenceMismatch);
    }
    let trusted_key = registry
        .get(&manifest.body.signer_id, &manifest.body.key_id)
        .ok_or(CheckpointError::UnknownKey)?;
    if manifest.body.issued_at_ms > now_ms {
        return Err(CheckpointError::CheckpointFromFuture);
    }
    if now_ms < trusted_key.valid_from_ms {
        return Err(CheckpointError::KeyNotYetValid);
    }
    if trusted_key
        .valid_until_ms
        .is_some_and(|until| now_ms > until)
    {
        return Err(CheckpointError::KeyExpired);
    }
    if trusted_key
        .revoked_at_ms
        .is_some_and(|revoked| now_ms >= revoked)
    {
        return Err(CheckpointError::KeyRevoked);
    }
    if manifest
        .body
        .expires_at_ms
        .is_some_and(|expires| now_ms > expires)
    {
        return Err(CheckpointError::CheckpointExpired);
    }

    let public_key = decode_fixed::<32>(&trusted_key.public_key_hex, "public_key_hex")?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| CheckpointError::InvalidHex("public_key_hex"))?;
    let signature_bytes = decode_hex(&manifest.signature_hex, "signature_hex")?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| CheckpointError::InvalidHex("signature_hex"))?;
    verifying_key
        .verify(&message, &signature)
        .map_err(|_| CheckpointError::SignatureVerificationFailed)
}

pub fn verify_checkpoint_chain(
    manifests: &[SignedCheckpointManifest],
    registry: &TrustedKeyRegistry,
    latest_anchor: Option<&ExternalAnchor>,
    now_ms: u64,
) -> Result<VerifiedCheckpointChain, CheckpointError> {
    if manifests.is_empty() {
        return Err(CheckpointError::EmptyChain);
    }
    for manifest in manifests {
        verify_signed_checkpoint(manifest, registry, now_ms)?;
    }

    let first = &manifests[0];
    for pair in manifests.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if current.body.ledger_profile != previous.body.ledger_profile
            || current.body.storage_root_identity != previous.body.storage_root_identity
            || current.body.signer_id != previous.body.signer_id
        {
            return Err(CheckpointError::ChainIdentityMismatch);
        }
        if current.body.previous_checkpoint_ref.as_deref() != Some(previous.manifest_ref.as_str()) {
            return Err(CheckpointError::PreviousCheckpointMismatch);
        }
        if current.body.last_sequence <= previous.body.last_sequence
            || (current.body.wal_segment, current.body.wal_offset)
                <= (previous.body.wal_segment, previous.body.wal_offset)
            || current.body.issued_at_ms < previous.body.issued_at_ms
        {
            return Err(CheckpointError::NonMonotonicCheckpoint);
        }
        if current.body.event_chain_head == previous.body.event_chain_head {
            return Err(CheckpointError::LedgerHeadNotAdvanced);
        }
    }

    let latest = manifests.last().expect("non-empty chain");
    let (status, external_anchor_id) = if let Some(anchor) = latest_anchor {
        validate_anchor(anchor)?;
        if latest.body.last_sequence < anchor.last_sequence {
            return Err(CheckpointError::ExternalAnchorRollback);
        }
        let anchored = manifests
            .iter()
            .find(|manifest| manifest.manifest_ref == anchor.checkpoint_ref);
        let anchored = match anchored {
            Some(anchored) => anchored,
            None => {
                if manifests.iter().any(|manifest| {
                    manifest.body.last_sequence == anchor.last_sequence
                        && (manifest.body.event_chain_head != anchor.event_chain_head
                            || manifest.body.storage_root_identity != anchor.storage_root_identity)
                }) {
                    return Err(CheckpointError::ExternalAnchorFork);
                }
                return Err(CheckpointError::ExternalAnchorNotInChain);
            }
        };
        if anchored.body.storage_root_identity != anchor.storage_root_identity
            || anchored.body.event_chain_head != anchor.event_chain_head
            || anchored.body.last_sequence != anchor.last_sequence
        {
            return Err(CheckpointError::ExternalAnchorFork);
        }
        (
            AntiRollbackStatus::ExternalAnchorVerified,
            Some(anchor.anchor_id.clone()),
        )
    } else {
        (AntiRollbackStatus::LocalSignatureOnly, None)
    };

    Ok(VerifiedCheckpointChain {
        status,
        storage_root_identity: latest.body.storage_root_identity.clone(),
        first_checkpoint_ref: first.manifest_ref.clone(),
        latest_checkpoint_ref: latest.manifest_ref.clone(),
        latest_event_chain_head: latest.body.event_chain_head.clone(),
        latest_sequence: latest.body.last_sequence,
        checkpoint_count: manifests.len(),
        external_anchor_id,
    })
}

fn validate_material(material: &CheckpointMaterial) -> Result<(), CheckpointError> {
    normalized(material.ledger_profile.clone(), "ledger_profile")?;
    validate_ref(&material.storage_root_identity, "storage_root_identity")?;
    validate_ref(&material.event_chain_head, "event_chain_head")?;
    validate_ref(&material.projection_digest, "projection_digest")?;
    validate_ref(&material.snapshot_digest, "snapshot_digest")?;
    if material.last_sequence == 0 {
        return Err(CheckpointError::InvalidField("last_sequence"));
    }
    Ok(())
}

fn validate_manifest_body(body: &CheckpointManifestBody) -> Result<(), CheckpointError> {
    if body.schema != CHECKPOINT_SCHEMA || body.checkpoint_profile != CHECKPOINT_PROFILE {
        return Err(CheckpointError::InvalidField("schema_or_profile"));
    }
    validate_material(&CheckpointMaterial {
        ledger_profile: body.ledger_profile.clone(),
        storage_root_identity: body.storage_root_identity.clone(),
        event_chain_head: body.event_chain_head.clone(),
        last_sequence: body.last_sequence,
        wal_segment: body.wal_segment,
        wal_offset: body.wal_offset,
        projection_digest: body.projection_digest.clone(),
        snapshot_digest: body.snapshot_digest.clone(),
    })?;
    normalized(body.signer_id.clone(), "signer_id")?;
    normalized(body.key_id.clone(), "key_id")?;
    if let Some(expires) = body.expires_at_ms {
        if expires <= body.issued_at_ms {
            return Err(CheckpointError::InvalidExpiry);
        }
    }
    if let Some(reference) = &body.previous_checkpoint_ref {
        validate_ref(reference, "previous_checkpoint_ref")?;
    }
    Ok(())
}

fn validate_trusted_key(key: &TrustedCheckpointKey) -> Result<(), CheckpointError> {
    normalized(key.signer_id.clone(), "signer_id")?;
    normalized(key.key_id.clone(), "key_id")?;
    decode_fixed::<32>(&key.public_key_hex, "public_key_hex")?;
    if key
        .valid_until_ms
        .is_some_and(|until| until < key.valid_from_ms)
    {
        return Err(CheckpointError::InvalidField("valid_until_ms"));
    }
    Ok(())
}

fn validate_anchor(anchor: &ExternalAnchor) -> Result<(), CheckpointError> {
    normalized(anchor.provider_profile.clone(), "provider_profile")?;
    normalized(anchor.anchor_id.clone(), "anchor_id")?;
    validate_ref(&anchor.checkpoint_ref, "checkpoint_ref")?;
    validate_ref(&anchor.storage_root_identity, "storage_root_identity")?;
    validate_ref(&anchor.event_chain_head, "event_chain_head")?;
    if anchor.last_sequence == 0 {
        return Err(CheckpointError::InvalidField("last_sequence"));
    }
    Ok(())
}

fn normalized(value: String, label: &'static str) -> Result<String, CheckpointError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        return Err(CheckpointError::InvalidField(label));
    }
    Ok(normalized)
}

fn validate_ref(reference: &str, label: &'static str) -> Result<(), CheckpointError> {
    let bytes = reference.as_bytes();
    let valid = bytes.len() == 71
        && reference.starts_with("sha256:")
        && bytes[7..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(CheckpointError::InvalidReference(label));
    }
    Ok(())
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CheckpointError> {
    serde_cbor::to_vec(value).map_err(|error| CheckpointError::Encoding(error.to_string()))
}

fn key_slot(signer_id: &str, key_id: &str) -> String {
    format!("{signer_id}\0{key_id}")
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn decode_hex(value: &str, label: &'static str) -> Result<Vec<u8>, CheckpointError> {
    if value.len() % 2 != 0 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CheckpointError::InvalidHex(label));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| CheckpointError::InvalidHex(label))
        })
        .collect()
}

fn decode_fixed<const N: usize>(
    value: &str,
    label: &'static str,
) -> Result<[u8; N], CheckpointError> {
    let bytes = decode_hex(value, label)?;
    bytes
        .try_into()
        .map_err(|_| CheckpointError::InvalidHex(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct Fixture {
        profile: String,
        keys: Vec<FixtureKey>,
        cases: Vec<FixtureCase>,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureKey {
        name: String,
        signer_id: String,
        key_id: String,
        seed_hex: String,
        valid_from_ms: u64,
        valid_until_ms: Option<u64>,
        revoked_at_ms: Option<u64>,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureCase {
        case_id: String,
        scenario: String,
        expected: String,
    }

    fn reference(label: &str) -> String {
        sha256_ref(label.as_bytes())
    }

    fn material(sequence: u64, head: &str) -> CheckpointMaterial {
        CheckpointMaterial {
            ledger_profile: LEDGER_PROFILE.to_owned(),
            storage_root_identity: reference("storage-root"),
            event_chain_head: reference(head),
            last_sequence: sequence,
            wal_segment: 1,
            wal_offset: sequence * 100,
            projection_digest: reference(&format!("projection-{sequence}")),
            snapshot_digest: reference(&format!("snapshot-{sequence}")),
        }
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../tests/fixtures/signed-checkpoint-anti-rollback-v0.1.json"
        ))
        .expect("fixture")
    }

    fn signer(key: &FixtureKey) -> CheckpointSigner {
        CheckpointSigner::from_seed_hex(key.signer_id.clone(), key.key_id.clone(), &key.seed_hex)
            .expect("signer")
    }

    fn registry(keys: &[&FixtureKey]) -> TrustedKeyRegistry {
        let mut registry = TrustedKeyRegistry::default();
        for key in keys {
            registry
                .insert(signer(key).trusted_key(
                    key.valid_from_ms,
                    key.valid_until_ms,
                    key.revoked_at_ms,
                ))
                .expect("trusted key");
        }
        registry
    }

    fn key<'a>(fixture: &'a Fixture, name: &str) -> &'a FixtureKey {
        fixture
            .keys
            .iter()
            .find(|key| key.name == name)
            .expect("fixture key")
    }

    #[test]
    fn deterministic_fixture_covers_required_anti_rollback_cases() {
        let fixture = fixture();
        assert_eq!(
            fixture.profile,
            "org.liminaldb.signed-checkpoint-fixture.v0.1"
        );
        let trusted = key(&fixture, "trusted-old");
        let rotated = key(&fixture, "trusted-new");
        let revoked = key(&fixture, "revoked");
        let attacker = key(&fixture, "attacker");

        for case in &fixture.cases {
            let outcome = match case.scenario.as_str() {
                "valid_signature" => {
                    let checkpoint = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[trusted]), None, 200)
                        .map(|result| format!("{:?}", result.status).to_uppercase())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "wrong_signer" => {
                    let checkpoint = signer(attacker)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[trusted]), None, 200)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "rotated_key" => {
                    let first = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("first");
                    let second = signer(rotated)
                        .sign(
                            material(20, "head-20"),
                            200,
                            Some(600),
                            Some(first.manifest_ref.clone()),
                        )
                        .expect("second");
                    verify_checkpoint_chain(
                        &[first, second],
                        &registry(&[trusted, rotated]),
                        None,
                        250,
                    )
                    .map(|result| format!("{:?}", result.status).to_uppercase())
                    .unwrap_or_else(|error| error.code().to_owned())
                }
                "revoked_key" => {
                    let checkpoint = signer(revoked)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[revoked]), None, 250)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "expired_key" => {
                    let expired = key(&fixture, "expired");
                    let checkpoint = signer(expired)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[expired]), None, 250)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "future_issued_checkpoint" => {
                    let checkpoint = signer(trusted)
                        .sign(material(10, "head-10"), 300, Some(500), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[trusted]), None, 250)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "stale_checkpoint" => {
                    let checkpoint = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(150), None)
                        .expect("sign");
                    verify_checkpoint_chain(&[checkpoint], &registry(&[trusted]), None, 200)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "forked_ledger_head" => {
                    let anchored = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("anchored");
                    let fork = signer(trusted)
                        .sign(material(10, "fork-head-10"), 100, Some(500), None)
                        .expect("fork");
                    let anchor = ExternalAnchor {
                        provider_profile: "example.immutable-registry.v0.1".to_owned(),
                        anchor_id: "anchor-10".to_owned(),
                        checkpoint_ref: anchored.manifest_ref,
                        storage_root_identity: anchored.body.storage_root_identity,
                        event_chain_head: anchored.body.event_chain_head,
                        last_sequence: anchored.body.last_sequence,
                        anchored_at_ms: 150,
                    };
                    verify_checkpoint_chain(&[fork], &registry(&[trusted]), Some(&anchor), 200)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "external_anchor_rollback" => {
                    let first = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("first");
                    let second = signer(trusted)
                        .sign(
                            material(20, "head-20"),
                            200,
                            Some(600),
                            Some(first.manifest_ref.clone()),
                        )
                        .expect("second");
                    let anchor = ExternalAnchor {
                        provider_profile: "example.immutable-registry.v0.1".to_owned(),
                        anchor_id: "anchor-20".to_owned(),
                        checkpoint_ref: second.manifest_ref,
                        storage_root_identity: second.body.storage_root_identity,
                        event_chain_head: second.body.event_chain_head,
                        last_sequence: second.body.last_sequence,
                        anchored_at_ms: 250,
                    };
                    verify_checkpoint_chain(&[first], &registry(&[trusted]), Some(&anchor), 300)
                        .map(|_| "VERIFIED".to_owned())
                        .unwrap_or_else(|error| error.code().to_owned())
                }
                "valid_external_anchor" => {
                    let first = signer(trusted)
                        .sign(material(10, "head-10"), 100, Some(500), None)
                        .expect("first");
                    let second = signer(rotated)
                        .sign(
                            material(20, "head-20"),
                            200,
                            Some(600),
                            Some(first.manifest_ref.clone()),
                        )
                        .expect("second");
                    let anchor = ExternalAnchor {
                        provider_profile: "example.immutable-registry.v0.1".to_owned(),
                        anchor_id: "anchor-10".to_owned(),
                        checkpoint_ref: first.manifest_ref.clone(),
                        storage_root_identity: first.body.storage_root_identity.clone(),
                        event_chain_head: first.body.event_chain_head.clone(),
                        last_sequence: first.body.last_sequence,
                        anchored_at_ms: 150,
                    };
                    verify_checkpoint_chain(
                        &[first, second],
                        &registry(&[trusted, rotated]),
                        Some(&anchor),
                        250,
                    )
                    .map(|result| format!("{:?}", result.status).to_uppercase())
                    .unwrap_or_else(|error| error.code().to_owned())
                }
                other => panic!("unknown fixture scenario: {other}"),
            };
            assert_eq!(outcome, case.expected, "case {}", case.case_id);
        }
    }

    #[test]
    fn tampering_with_signed_body_is_detected() {
        let fixture = fixture();
        let trusted = key(&fixture, "trusted-old");
        let mut checkpoint = signer(trusted)
            .sign(material(10, "head-10"), 100, Some(500), None)
            .expect("sign");
        checkpoint.body.last_sequence = 11;
        let error = verify_signed_checkpoint(&checkpoint, &registry(&[trusted]), 200)
            .expect_err("tampered body must fail");
        assert_eq!(error, CheckpointError::ManifestReferenceMismatch);
    }
}
