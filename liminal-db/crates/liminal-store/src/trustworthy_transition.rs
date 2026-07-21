use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::wal::{Offset, Store};

const EVENT_SCHEMA: &str = "liminaldb.trustworthy-transition-event.v0.1";
const SNAPSHOT_SCHEMA: &str = "liminaldb.trustworthy-transition-snapshot.v0.1";
const PROFILE: &str = "org.liminaldb.trustworthy-transition-ledger.v0.1";
const SNAPSHOT_FILE: &str = "trustworthy-transition-v0.1.snap";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionRecordKind {
    Authorization,
    Observation,
    ResponseIntegrity,
    CausalAudit,
    ContinuitySnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorityState {
    Valid,
    Denied,
    Pending,
    Expired,
    ExpiredAtReport,
    Consumed,
    RevalidationRequired,
    NotEvaluated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionState {
    NotObserved,
    ObservedExecuted,
    ObservedBlocked,
    ObservedErrored,
    ObservedOther,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResponseIntegrityState {
    Verified,
    Failed,
    Partial,
    NotEvaluated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CausalValidityState {
    Valid,
    Invalid,
    NotEvaluated,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ContinuityPosture {
    ContinueSideEffect,
    RetrySideEffect,
    ReportOnly,
    RemediateResponse,
    Revalidate,
    Blocked,
    AlreadyConsumed,
    NotEvaluated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionDimensions {
    pub authority: AuthorityState,
    pub execution: ExecutionState,
    pub response_integrity: ResponseIntegrityState,
    pub causal_validity: CausalValidityState,
    pub continuity_posture: ContinuityPosture,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionLinks {
    pub authorization_ref: Option<String>,
    pub observation_refs: Vec<String>,
    pub response_integrity_ref: Option<String>,
    pub causal_audit_ref: Option<String>,
    pub previous_continuity_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionEventInput {
    pub transition_id: String,
    pub subject_id: String,
    pub kind: TransitionRecordKind,
    pub record_ref: String,
    pub payload_digest: String,
    pub links: TransitionLinks,
    pub dimensions: Option<TransitionDimensions>,
    pub side_effect_committed: Option<bool>,
    pub captured_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionEventBody {
    pub schema: String,
    pub profile: String,
    pub sequence: u64,
    pub transition_id: String,
    pub subject_id: String,
    pub kind: TransitionRecordKind,
    pub record_ref: String,
    pub payload_digest: String,
    pub links: TransitionLinks,
    pub dimensions: Option<TransitionDimensions>,
    pub side_effect_committed: Option<bool>,
    pub captured_at_ms: u64,
    pub previous_event_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionEvent {
    pub body: TransitionEventBody,
    pub event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionProjection {
    pub transition_id: String,
    pub subject_id: String,
    pub authorization_ref: Option<String>,
    pub authorization_epoch: u64,
    pub observation_refs: Vec<String>,
    pub response_integrity_ref: Option<String>,
    pub causal_audit_ref: Option<String>,
    pub continuity_snapshot_ref: Option<String>,
    pub dimensions: Option<TransitionDimensions>,
    pub side_effect_committed: bool,
    pub last_sequence: u64,
    pub last_event_hash: String,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordOwner {
    transition_id: String,
    subject_id: String,
    kind: TransitionRecordKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LedgerState {
    next_sequence: u64,
    head_event_hash: Option<String>,
    projections: BTreeMap<String, TransitionProjection>,
    record_owners: BTreeMap<String, RecordOwner>,
}

impl Default for LedgerState {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            head_event_hash: None,
            projections: BTreeMap::new(),
            record_owners: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotOffset {
    segment: u64,
    position: u64,
}

impl From<Offset> for SnapshotOffset {
    fn from(value: Offset) -> Self {
        Self {
            segment: value.segment,
            position: value.position,
        }
    }
}

impl From<SnapshotOffset> for Offset {
    fn from(value: SnapshotOffset) -> Self {
        Offset {
            segment: value.segment,
            position: value.position,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TransitionLedgerSnapshotBody {
    schema: String,
    profile: String,
    wal_offset: SnapshotOffset,
    next_sequence: u64,
    head_event_hash: Option<String>,
    projections: BTreeMap<String, TransitionProjection>,
    record_owners: BTreeMap<String, RecordOwner>,
    projection_digest: String,
    created_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TransitionLedgerSnapshot {
    body: TransitionLedgerSnapshotBody,
    snapshot_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionLedgerSnapshotInfo {
    pub path: PathBuf,
    pub offset: Offset,
    pub event_count: u64,
    pub projection_count: usize,
    pub snapshot_digest: String,
}

#[derive(Debug, Error)]
pub enum TransitionLedgerError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("invalid identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid sha256 reference in {0}")]
    InvalidReference(&'static str),
    #[error("event schema or profile mismatch")]
    EventProfileMismatch,
    #[error("snapshot schema or profile mismatch")]
    SnapshotProfileMismatch,
    #[error("event sequence mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    #[error("event previous hash does not match the ledger head")]
    PreviousEventHashMismatch,
    #[error("event hash mismatch")]
    EventHashMismatch,
    #[error("snapshot digest mismatch")]
    SnapshotDigestMismatch,
    #[error("snapshot projection digest mismatch")]
    SnapshotProjectionDigestMismatch,
    #[error("snapshot-assisted replay differs from full WAL replay")]
    ReplayProjectionMismatch,
    #[error("duplicate record reference: {0}")]
    DuplicateRecordReference(String),
    #[error("transition has no authorization root: {0}")]
    MissingAuthorization(String),
    #[error("missing parent record: {0}")]
    MissingParent(String),
    #[error("parent link mismatch: {0}")]
    ParentMismatch(&'static str),
    #[error("cross-transition or cross-subject reference: {0}")]
    CrossTransitionReference(String),
    #[error("subject mismatch for transition {0}")]
    SubjectMismatch(String),
    #[error("authorization replacement must explicitly supersede the current authorization")]
    ReauthorizationWithoutSupersession,
    #[error("observation evidence set mismatch")]
    ObservationSetMismatch,
    #[error("side_effect_committed cannot roll back from true to false")]
    SideEffectRollback,
    #[error("execution cannot roll back from OBSERVED_EXECUTED")]
    ExecutionRollback,
    #[error("continuity snapshots must carry all independent dimensions")]
    ContinuityDimensionsRequired,
    #[error("invalid links for record kind {0:?}")]
    InvalidLinks(TransitionRecordKind),
    #[error(
        "ledger is poisoned after an ambiguous storage failure; reopen and replay before appending"
    )]
    PoisonedAfterStorageFailure,
}

pub struct TrustworthyTransitionLedger {
    store: Store,
    state: LedgerState,
    snapshot_path: PathBuf,
    poisoned: bool,
}

impl TrustworthyTransitionLedger {
    /// Opens a dedicated trustworthy-transition ledger root and verifies both
    /// snapshot-assisted replay and full WAL replay before returning.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, TransitionLedgerError> {
        let store = Store::open(path.as_ref()).map_err(storage_error)?;
        let snapshot_path = store.snap_dir().join(SNAPSHOT_FILE);

        let snapshot_recovered = if snapshot_path.exists() {
            let snapshot = load_and_verify_snapshot(&snapshot_path)?;
            let initial = LedgerState {
                next_sequence: snapshot.body.next_sequence,
                head_event_hash: snapshot.body.head_event_hash.clone(),
                projections: snapshot.body.projections.clone(),
                record_owners: snapshot.body.record_owners.clone(),
            };
            replay_from(&store, snapshot.body.wal_offset.into(), initial)?
        } else {
            replay_from(&store, Offset::start(), LedgerState::default())?
        };

        let full_replay = replay_from(&store, Offset::start(), LedgerState::default())?;
        if snapshot_recovered != full_replay {
            return Err(TransitionLedgerError::ReplayProjectionMismatch);
        }

        Ok(Self {
            store,
            state: full_replay,
            snapshot_path,
            poisoned: false,
        })
    }

    /// Appends one semantic transition event after validating it against a
    /// cloned deterministic projection. WAL bytes are synchronized before the
    /// in-memory projection is advanced.
    pub fn append(
        &mut self,
        input: TransitionEventInput,
    ) -> Result<TransitionEvent, TransitionLedgerError> {
        if self.poisoned {
            return Err(TransitionLedgerError::PoisonedAfterStorageFailure);
        }
        let normalized = normalize_input(input)?;
        let body = TransitionEventBody {
            schema: EVENT_SCHEMA.to_owned(),
            profile: PROFILE.to_owned(),
            sequence: self.state.next_sequence,
            transition_id: normalized.transition_id,
            subject_id: normalized.subject_id,
            kind: normalized.kind,
            record_ref: normalized.record_ref,
            payload_digest: normalized.payload_digest,
            links: normalized.links,
            dimensions: normalized.dimensions,
            side_effect_committed: normalized.side_effect_committed,
            captured_at_ms: normalized.captured_at_ms,
            previous_event_hash: self.state.head_event_hash.clone(),
        };
        let event_hash = digest_cbor(&body)?;
        let event = TransitionEvent { body, event_hash };

        let mut candidate = self.state.clone();
        apply_event(&mut candidate, &event)?;
        let bytes = serde_cbor::to_vec(&event).map_err(encoding_error)?;
        if let Err(error) = self.store.append(&bytes) {
            self.poisoned = true;
            return Err(storage_error(error));
        }
        self.state = candidate;
        Ok(event)
    }

    pub fn projection(&self, transition_id: &str) -> Option<&TransitionProjection> {
        self.state.projections.get(transition_id)
    }

    pub fn projections(&self) -> &BTreeMap<String, TransitionProjection> {
        &self.state.projections
    }

    pub fn head_event_hash(&self) -> Option<&str> {
        self.state.head_event_hash.as_deref()
    }

    pub fn event_count(&self) -> u64 {
        self.state.next_sequence.saturating_sub(1)
    }

    /// Writes an atomically replaced, digest-bound snapshot at the current WAL
    /// offset. The snapshot remains an accelerator; open() still compares it
    /// with a full replay from offset zero.
    pub fn write_snapshot(
        &mut self,
        created_at_ms: u64,
    ) -> Result<TransitionLedgerSnapshotInfo, TransitionLedgerError> {
        let projection_digest = projection_digest(&self.state)?;
        let body = TransitionLedgerSnapshotBody {
            schema: SNAPSHOT_SCHEMA.to_owned(),
            profile: PROFILE.to_owned(),
            wal_offset: self.store.end_offset().into(),
            next_sequence: self.state.next_sequence,
            head_event_hash: self.state.head_event_hash.clone(),
            projections: self.state.projections.clone(),
            record_owners: self.state.record_owners.clone(),
            projection_digest,
            created_at_ms,
        };
        let snapshot_digest = digest_cbor(&body)?;
        let snapshot = TransitionLedgerSnapshot {
            body,
            snapshot_digest: snapshot_digest.clone(),
        };
        let bytes = serde_cbor::to_vec(&snapshot).map_err(encoding_error)?;
        atomic_write(&self.snapshot_path, &bytes)?;

        Ok(TransitionLedgerSnapshotInfo {
            path: self.snapshot_path.clone(),
            offset: snapshot.body.wal_offset.into(),
            event_count: self.event_count(),
            projection_count: self.state.projections.len(),
            snapshot_digest,
        })
    }

    pub fn snapshot_path(&self) -> &Path {
        &self.snapshot_path
    }
}

pub fn sha256_ref(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(71);
    output.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn normalize_input(
    mut input: TransitionEventInput,
) -> Result<TransitionEventInput, TransitionLedgerError> {
    input.transition_id = non_empty(input.transition_id, "transition_id")?;
    input.subject_id = non_empty(input.subject_id, "subject_id")?;
    validate_ref(&input.record_ref, "record_ref")?;
    validate_ref(&input.payload_digest, "payload_digest")?;
    validate_optional_ref(&input.links.authorization_ref, "authorization_ref")?;
    validate_optional_ref(
        &input.links.response_integrity_ref,
        "response_integrity_ref",
    )?;
    validate_optional_ref(&input.links.causal_audit_ref, "causal_audit_ref")?;
    validate_optional_ref(
        &input.links.previous_continuity_ref,
        "previous_continuity_ref",
    )?;
    for reference in &input.links.observation_refs {
        validate_ref(reference, "observation_ref")?;
    }
    input.links.observation_refs.sort();
    input.links.observation_refs.dedup();
    if input.kind == TransitionRecordKind::ContinuitySnapshot && input.dimensions.is_none() {
        return Err(TransitionLedgerError::ContinuityDimensionsRequired);
    }
    Ok(input)
}

fn validate_event_body(body: &TransitionEventBody) -> Result<(), TransitionLedgerError> {
    validate_canonical_identifier(&body.transition_id, "transition_id")?;
    validate_canonical_identifier(&body.subject_id, "subject_id")?;
    validate_ref(&body.record_ref, "record_ref")?;
    validate_ref(&body.payload_digest, "payload_digest")?;
    validate_optional_ref(&body.links.authorization_ref, "authorization_ref")?;
    validate_optional_ref(&body.links.response_integrity_ref, "response_integrity_ref")?;
    validate_optional_ref(&body.links.causal_audit_ref, "causal_audit_ref")?;
    validate_optional_ref(
        &body.links.previous_continuity_ref,
        "previous_continuity_ref",
    )?;
    validate_optional_ref(&body.previous_event_hash, "previous_event_hash")?;
    for reference in &body.links.observation_refs {
        validate_ref(reference, "observation_ref")?;
    }
    let mut canonical_observations = body.links.observation_refs.clone();
    canonical_observations.sort();
    canonical_observations.dedup();
    if canonical_observations != body.links.observation_refs {
        return Err(TransitionLedgerError::InvalidLinks(body.kind));
    }
    if body.kind == TransitionRecordKind::ContinuitySnapshot && body.dimensions.is_none() {
        return Err(TransitionLedgerError::ContinuityDimensionsRequired);
    }
    Ok(())
}

fn validate_canonical_identifier(
    value: &str,
    label: &'static str,
) -> Result<(), TransitionLedgerError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed != value {
        return Err(TransitionLedgerError::InvalidIdentifier(label));
    }
    Ok(())
}

fn non_empty(value: String, label: &'static str) -> Result<String, TransitionLedgerError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        return Err(TransitionLedgerError::InvalidIdentifier(label));
    }
    Ok(normalized)
}

fn validate_optional_ref(
    reference: &Option<String>,
    label: &'static str,
) -> Result<(), TransitionLedgerError> {
    if let Some(reference) = reference {
        validate_ref(reference, label)?;
    }
    Ok(())
}

fn validate_ref(reference: &str, label: &'static str) -> Result<(), TransitionLedgerError> {
    let bytes = reference.as_bytes();
    let valid = bytes.len() == 71
        && reference.starts_with("sha256:")
        && bytes[7..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(TransitionLedgerError::InvalidReference(label));
    }
    Ok(())
}

fn replay_from(
    store: &Store,
    offset: Offset,
    mut state: LedgerState,
) -> Result<LedgerState, TransitionLedgerError> {
    let stream = store.stream_from(offset).map_err(storage_error)?;
    for record in stream {
        let bytes = record.map_err(storage_error)?;
        let event: TransitionEvent = serde_cbor::from_slice(&bytes).map_err(encoding_error)?;
        apply_event(&mut state, &event)?;
    }
    Ok(state)
}

fn apply_event(
    state: &mut LedgerState,
    event: &TransitionEvent,
) -> Result<(), TransitionLedgerError> {
    validate_event_body(&event.body)?;
    if event.body.schema != EVENT_SCHEMA || event.body.profile != PROFILE {
        return Err(TransitionLedgerError::EventProfileMismatch);
    }
    if event.body.sequence != state.next_sequence {
        return Err(TransitionLedgerError::SequenceMismatch {
            expected: state.next_sequence,
            actual: event.body.sequence,
        });
    }
    if event.body.previous_event_hash != state.head_event_hash {
        return Err(TransitionLedgerError::PreviousEventHashMismatch);
    }
    if digest_cbor(&event.body)? != event.event_hash {
        return Err(TransitionLedgerError::EventHashMismatch);
    }
    validate_ref(&event.body.record_ref, "record_ref")?;
    validate_ref(&event.body.payload_digest, "payload_digest")?;
    if state.record_owners.contains_key(&event.body.record_ref) {
        return Err(TransitionLedgerError::DuplicateRecordReference(
            event.body.record_ref.clone(),
        ));
    }

    match event.body.kind {
        TransitionRecordKind::Authorization => apply_authorization(state, event)?,
        _ => apply_dependent_record(state, event)?,
    }

    state.record_owners.insert(
        event.body.record_ref.clone(),
        RecordOwner {
            transition_id: event.body.transition_id.clone(),
            subject_id: event.body.subject_id.clone(),
            kind: event.body.kind,
        },
    );
    state.head_event_hash = Some(event.event_hash.clone());
    state.next_sequence += 1;
    Ok(())
}

fn apply_authorization(
    state: &mut LedgerState,
    event: &TransitionEvent,
) -> Result<(), TransitionLedgerError> {
    if !event.body.links.observation_refs.is_empty()
        || event.body.links.response_integrity_ref.is_some()
        || event.body.links.causal_audit_ref.is_some()
        || event.body.links.previous_continuity_ref.is_some()
    {
        return Err(TransitionLedgerError::InvalidLinks(event.body.kind));
    }

    if let Some(existing) = state.projections.get(&event.body.transition_id).cloned() {
        if existing.subject_id != event.body.subject_id {
            return Err(TransitionLedgerError::SubjectMismatch(
                event.body.transition_id.clone(),
            ));
        }
        let expected = existing.authorization_ref.clone().ok_or_else(|| {
            TransitionLedgerError::MissingAuthorization(event.body.transition_id.clone())
        })?;
        if event.body.links.authorization_ref.as_deref() != Some(expected.as_str()) {
            return Err(TransitionLedgerError::ReauthorizationWithoutSupersession);
        }
        ensure_owned_by(
            state,
            &expected,
            &event.body.transition_id,
            &event.body.subject_id,
        )?;
        validate_monotonic_dimensions(
            existing.dimensions.as_ref(),
            event.body.dimensions.as_ref(),
        )?;
        validate_side_effect(
            existing.side_effect_committed,
            event.body.side_effect_committed,
        )?;

        let projection = state
            .projections
            .get_mut(&event.body.transition_id)
            .expect("projection exists after immutable lookup");
        projection.authorization_ref = Some(event.body.record_ref.clone());
        projection.authorization_epoch += 1;
        projection.observation_refs.clear();
        projection.response_integrity_ref = None;
        projection.causal_audit_ref = None;
        projection.continuity_snapshot_ref = None;
        if let Some(dimensions) = &event.body.dimensions {
            projection.dimensions = Some(dimensions.clone());
        }
        if event.body.side_effect_committed == Some(true) {
            projection.side_effect_committed = true;
        }
        update_projection_tail(projection, event);
        return Ok(());
    }

    if event.body.links.authorization_ref.is_some() {
        return Err(TransitionLedgerError::InvalidLinks(event.body.kind));
    }
    let projection = TransitionProjection {
        transition_id: event.body.transition_id.clone(),
        subject_id: event.body.subject_id.clone(),
        authorization_ref: Some(event.body.record_ref.clone()),
        authorization_epoch: 1,
        observation_refs: Vec::new(),
        response_integrity_ref: None,
        causal_audit_ref: None,
        continuity_snapshot_ref: None,
        dimensions: event.body.dimensions.clone(),
        side_effect_committed: event.body.side_effect_committed.unwrap_or(false),
        last_sequence: event.body.sequence,
        last_event_hash: event.event_hash.clone(),
        event_count: 1,
    };
    state
        .projections
        .insert(event.body.transition_id.clone(), projection);
    Ok(())
}

fn apply_dependent_record(
    state: &mut LedgerState,
    event: &TransitionEvent,
) -> Result<(), TransitionLedgerError> {
    let current = state
        .projections
        .get(&event.body.transition_id)
        .cloned()
        .ok_or_else(|| {
            TransitionLedgerError::MissingAuthorization(event.body.transition_id.clone())
        })?;
    if current.subject_id != event.body.subject_id {
        return Err(TransitionLedgerError::SubjectMismatch(
            event.body.transition_id.clone(),
        ));
    }

    let expected_authorization = current.authorization_ref.clone().ok_or_else(|| {
        TransitionLedgerError::MissingAuthorization(event.body.transition_id.clone())
    })?;
    expect_optional_ref(
        state,
        &event.body.links.authorization_ref,
        &Some(expected_authorization),
        "authorization_ref",
        event,
    )?;

    match event.body.kind {
        TransitionRecordKind::Authorization => unreachable!("handled by apply_authorization"),
        TransitionRecordKind::Observation => {
            if !event.body.links.observation_refs.is_empty()
                || event.body.links.response_integrity_ref.is_some()
                || event.body.links.causal_audit_ref.is_some()
                || event.body.links.previous_continuity_ref.is_some()
            {
                return Err(TransitionLedgerError::InvalidLinks(event.body.kind));
            }
        }
        TransitionRecordKind::ResponseIntegrity => {
            expect_observation_set(state, &event.body.links.observation_refs, &current, event)?;
            if event.body.links.response_integrity_ref.is_some()
                || event.body.links.causal_audit_ref.is_some()
                || event.body.links.previous_continuity_ref.is_some()
            {
                return Err(TransitionLedgerError::InvalidLinks(event.body.kind));
            }
        }
        TransitionRecordKind::CausalAudit => {
            expect_observation_set(state, &event.body.links.observation_refs, &current, event)?;
            expect_optional_ref(
                state,
                &event.body.links.response_integrity_ref,
                &current.response_integrity_ref,
                "response_integrity_ref",
                event,
            )?;
            if event.body.links.causal_audit_ref.is_some()
                || event.body.links.previous_continuity_ref.is_some()
            {
                return Err(TransitionLedgerError::InvalidLinks(event.body.kind));
            }
        }
        TransitionRecordKind::ContinuitySnapshot => {
            expect_observation_set(state, &event.body.links.observation_refs, &current, event)?;
            expect_optional_ref(
                state,
                &event.body.links.response_integrity_ref,
                &current.response_integrity_ref,
                "response_integrity_ref",
                event,
            )?;
            expect_optional_ref(
                state,
                &event.body.links.causal_audit_ref,
                &current.causal_audit_ref,
                "causal_audit_ref",
                event,
            )?;
            expect_optional_ref(
                state,
                &event.body.links.previous_continuity_ref,
                &current.continuity_snapshot_ref,
                "previous_continuity_ref",
                event,
            )?;
        }
    }

    validate_monotonic_dimensions(current.dimensions.as_ref(), event.body.dimensions.as_ref())?;
    validate_side_effect(
        current.side_effect_committed,
        event.body.side_effect_committed,
    )?;

    let projection = state
        .projections
        .get_mut(&event.body.transition_id)
        .expect("projection exists after immutable lookup");
    match event.body.kind {
        TransitionRecordKind::Authorization => unreachable!("handled by apply_authorization"),
        TransitionRecordKind::Observation => {
            projection
                .observation_refs
                .push(event.body.record_ref.clone());
            projection.observation_refs.sort();
            projection.observation_refs.dedup();
            projection.response_integrity_ref = None;
            projection.causal_audit_ref = None;
            projection.continuity_snapshot_ref = None;
        }
        TransitionRecordKind::ResponseIntegrity => {
            projection.response_integrity_ref = Some(event.body.record_ref.clone());
            projection.causal_audit_ref = None;
            projection.continuity_snapshot_ref = None;
        }
        TransitionRecordKind::CausalAudit => {
            projection.causal_audit_ref = Some(event.body.record_ref.clone());
            projection.continuity_snapshot_ref = None;
        }
        TransitionRecordKind::ContinuitySnapshot => {
            projection.continuity_snapshot_ref = Some(event.body.record_ref.clone());
        }
    }
    if let Some(dimensions) = &event.body.dimensions {
        projection.dimensions = Some(dimensions.clone());
    }
    if event.body.side_effect_committed == Some(true) {
        projection.side_effect_committed = true;
    }
    update_projection_tail(projection, event);
    Ok(())
}

fn update_projection_tail(projection: &mut TransitionProjection, event: &TransitionEvent) {
    projection.last_sequence = event.body.sequence;
    projection.last_event_hash = event.event_hash.clone();
    projection.event_count += 1;
}

fn expect_observation_set(
    state: &LedgerState,
    actual: &[String],
    current: &TransitionProjection,
    event: &TransitionEvent,
) -> Result<(), TransitionLedgerError> {
    if actual != current.observation_refs.as_slice() {
        return Err(TransitionLedgerError::ObservationSetMismatch);
    }
    for reference in actual {
        ensure_owned_by(
            state,
            reference,
            &event.body.transition_id,
            &event.body.subject_id,
        )?;
    }
    Ok(())
}

fn expect_optional_ref(
    state: &LedgerState,
    actual: &Option<String>,
    expected: &Option<String>,
    label: &'static str,
    event: &TransitionEvent,
) -> Result<(), TransitionLedgerError> {
    if let Some(reference) = actual {
        if !state.record_owners.contains_key(reference) {
            return Err(TransitionLedgerError::MissingParent(reference.clone()));
        }
    }
    if actual != expected {
        return Err(TransitionLedgerError::ParentMismatch(label));
    }
    if let Some(reference) = actual {
        ensure_owned_by(
            state,
            reference,
            &event.body.transition_id,
            &event.body.subject_id,
        )?;
    }
    Ok(())
}

fn ensure_owned_by(
    state: &LedgerState,
    reference: &str,
    transition_id: &str,
    subject_id: &str,
) -> Result<(), TransitionLedgerError> {
    let owner = state
        .record_owners
        .get(reference)
        .ok_or_else(|| TransitionLedgerError::MissingParent(reference.to_owned()))?;
    if owner.transition_id != transition_id || owner.subject_id != subject_id {
        return Err(TransitionLedgerError::CrossTransitionReference(
            reference.to_owned(),
        ));
    }
    Ok(())
}

fn validate_side_effect(previous: bool, update: Option<bool>) -> Result<(), TransitionLedgerError> {
    if previous && update == Some(false) {
        return Err(TransitionLedgerError::SideEffectRollback);
    }
    Ok(())
}

fn validate_monotonic_dimensions(
    previous: Option<&TransitionDimensions>,
    update: Option<&TransitionDimensions>,
) -> Result<(), TransitionLedgerError> {
    if let (Some(previous), Some(update)) = (previous, update) {
        if previous.execution == ExecutionState::ObservedExecuted
            && update.execution != ExecutionState::ObservedExecuted
        {
            return Err(TransitionLedgerError::ExecutionRollback);
        }
    }
    Ok(())
}

fn load_and_verify_snapshot(
    path: &Path,
) -> Result<TransitionLedgerSnapshot, TransitionLedgerError> {
    let bytes = fs::read(path).map_err(storage_error)?;
    let snapshot: TransitionLedgerSnapshot =
        serde_cbor::from_slice(&bytes).map_err(encoding_error)?;
    if snapshot.body.schema != SNAPSHOT_SCHEMA || snapshot.body.profile != PROFILE {
        return Err(TransitionLedgerError::SnapshotProfileMismatch);
    }
    if digest_cbor(&snapshot.body)? != snapshot.snapshot_digest {
        return Err(TransitionLedgerError::SnapshotDigestMismatch);
    }
    let state = LedgerState {
        next_sequence: snapshot.body.next_sequence,
        head_event_hash: snapshot.body.head_event_hash.clone(),
        projections: snapshot.body.projections.clone(),
        record_owners: snapshot.body.record_owners.clone(),
    };
    if projection_digest(&state)? != snapshot.body.projection_digest {
        return Err(TransitionLedgerError::SnapshotProjectionDigestMismatch);
    }
    Ok(snapshot)
}

#[derive(Serialize)]
struct ProjectionDigestMaterial<'a> {
    next_sequence: u64,
    head_event_hash: &'a Option<String>,
    projections: &'a BTreeMap<String, TransitionProjection>,
    record_owners: &'a BTreeMap<String, RecordOwner>,
}

fn projection_digest(state: &LedgerState) -> Result<String, TransitionLedgerError> {
    digest_cbor(&ProjectionDigestMaterial {
        next_sequence: state.next_sequence,
        head_event_hash: &state.head_event_hash,
        projections: &state.projections,
        record_owners: &state.record_owners,
    })
}

fn digest_cbor<T: Serialize>(value: &T) -> Result<String, TransitionLedgerError> {
    let bytes = serde_cbor::to_vec(value).map_err(encoding_error)?;
    Ok(sha256_ref(&bytes))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), TransitionLedgerError> {
    let parent = path
        .parent()
        .ok_or_else(|| TransitionLedgerError::Storage("snapshot path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(storage_error)?;
    let temporary = parent.join(format!(".{}.{}.tmp", SNAPSHOT_FILE, std::process::id()));
    {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(storage_error)?;
        file.write_all(bytes).map_err(storage_error)?;
        file.sync_all().map_err(storage_error)?;
    }
    fs::rename(&temporary, path).map_err(storage_error)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(storage_error)?;
    Ok(())
}

fn storage_error(error: impl std::fmt::Display) -> TransitionLedgerError {
    TransitionLedgerError::Storage(error.to_string())
}

fn encoding_error(error: impl std::fmt::Display) -> TransitionLedgerError {
    TransitionLedgerError::Encoding(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn reference(label: &str) -> String {
        sha256_ref(label.as_bytes())
    }

    fn dimensions(
        execution: ExecutionState,
        integrity: ResponseIntegrityState,
        causal: CausalValidityState,
        posture: ContinuityPosture,
    ) -> TransitionDimensions {
        TransitionDimensions {
            authority: AuthorityState::Valid,
            execution,
            response_integrity: integrity,
            causal_validity: causal,
            continuity_posture: posture,
        }
    }

    fn input(
        transition_id: &str,
        subject_id: &str,
        kind: TransitionRecordKind,
        label: &str,
        links: TransitionLinks,
    ) -> TransitionEventInput {
        TransitionEventInput {
            transition_id: transition_id.to_owned(),
            subject_id: subject_id.to_owned(),
            kind,
            record_ref: reference(&format!("record:{label}")),
            payload_digest: reference(&format!("payload:{label}")),
            links,
            dimensions: None,
            side_effect_committed: None,
            captured_at_ms: 1,
        }
    }

    fn append_full_chain(
        ledger: &mut TrustworthyTransitionLedger,
        transition_id: &str,
        subject_id: &str,
    ) -> Vec<TransitionEvent> {
        let authorization = ledger
            .append(input(
                transition_id,
                subject_id,
                TransitionRecordKind::Authorization,
                "authorization",
                TransitionLinks::default(),
            ))
            .expect("authorization");

        let mut observation_input = input(
            transition_id,
            subject_id,
            TransitionRecordKind::Observation,
            "observation",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        observation_input.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::NotEvaluated,
            CausalValidityState::NotEvaluated,
            ContinuityPosture::NotEvaluated,
        ));
        observation_input.side_effect_committed = Some(true);
        let observation = ledger.append(observation_input).expect("observation");

        let mut integrity_input = input(
            transition_id,
            subject_id,
            TransitionRecordKind::ResponseIntegrity,
            "integrity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                ..TransitionLinks::default()
            },
        );
        integrity_input.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::NotEvaluated,
            ContinuityPosture::RemediateResponse,
        ));
        let integrity = ledger.append(integrity_input).expect("integrity");

        let mut causal_input = input(
            transition_id,
            subject_id,
            TransitionRecordKind::CausalAudit,
            "causal",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(integrity.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        causal_input.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::Valid,
            ContinuityPosture::RemediateResponse,
        ));
        let causal = ledger.append(causal_input).expect("causal");

        let mut continuity_input = input(
            transition_id,
            subject_id,
            TransitionRecordKind::ContinuitySnapshot,
            "continuity",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                observation_refs: vec![observation.body.record_ref.clone()],
                response_integrity_ref: Some(integrity.body.record_ref.clone()),
                causal_audit_ref: Some(causal.body.record_ref.clone()),
                previous_continuity_ref: None,
            },
        );
        continuity_input.dimensions = Some(dimensions(
            ExecutionState::ObservedExecuted,
            ResponseIntegrityState::Failed,
            CausalValidityState::Valid,
            ContinuityPosture::RemediateResponse,
        ));
        continuity_input.side_effect_committed = Some(true);
        let continuity = ledger.append(continuity_input).expect("continuity");

        vec![authorization, observation, integrity, causal, continuity]
    }

    #[test]
    fn full_chain_survives_snapshot_and_restart() {
        let directory = tempdir().expect("tempdir");
        let expected = {
            let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
            append_full_chain(&mut ledger, "transition-001", "agent:deploy");
            ledger.write_snapshot(100).expect("snapshot");
            ledger
                .projection("transition-001")
                .cloned()
                .expect("projection")
        };

        let reopened = TrustworthyTransitionLedger::open(directory.path()).expect("reopen");
        assert_eq!(reopened.event_count(), 5);
        assert_eq!(reopened.projection("transition-001"), Some(&expected));
        assert_eq!(reopened.projections().len(), 1);
    }

    #[test]
    fn cross_transition_parent_is_rejected() {
        let directory = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
        let authorization_a = ledger
            .append(input(
                "transition-a",
                "agent:a",
                TransitionRecordKind::Authorization,
                "auth-a",
                TransitionLinks::default(),
            ))
            .expect("auth a");
        ledger
            .append(input(
                "transition-b",
                "agent:b",
                TransitionRecordKind::Authorization,
                "auth-b",
                TransitionLinks::default(),
            ))
            .expect("auth b");

        let error = ledger
            .append(input(
                "transition-b",
                "agent:b",
                TransitionRecordKind::Observation,
                "obs-b",
                TransitionLinks {
                    authorization_ref: Some(authorization_a.body.record_ref),
                    ..TransitionLinks::default()
                },
            ))
            .expect_err("cross-transition parent must fail");
        assert!(matches!(error, TransitionLedgerError::ParentMismatch(_)));
    }

    #[test]
    fn exact_observation_set_is_required() {
        let directory = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
        let authorization = ledger
            .append(input(
                "transition-obs",
                "agent:obs",
                TransitionRecordKind::Authorization,
                "auth-obs",
                TransitionLinks::default(),
            ))
            .expect("authorization");
        ledger
            .append(input(
                "transition-obs",
                "agent:obs",
                TransitionRecordKind::Observation,
                "obs-1",
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref.clone()),
                    ..TransitionLinks::default()
                },
            ))
            .expect("observation");

        let error = ledger
            .append(input(
                "transition-obs",
                "agent:obs",
                TransitionRecordKind::ResponseIntegrity,
                "integrity-missing-obs",
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref),
                    observation_refs: Vec::new(),
                    ..TransitionLinks::default()
                },
            ))
            .expect_err("missing observation must fail");
        assert!(matches!(
            error,
            TransitionLedgerError::ObservationSetMismatch
        ));
    }

    #[test]
    fn record_references_are_globally_unique() {
        let directory = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
        let first = input(
            "transition-one",
            "agent:one",
            TransitionRecordKind::Authorization,
            "duplicate",
            TransitionLinks::default(),
        );
        let second = input(
            "transition-two",
            "agent:two",
            TransitionRecordKind::Authorization,
            "duplicate",
            TransitionLinks::default(),
        );
        ledger.append(first).expect("first");
        let error = ledger.append(second).expect_err("duplicate must fail");
        assert!(matches!(
            error,
            TransitionLedgerError::DuplicateRecordReference(_)
        ));
    }

    #[test]
    fn side_effect_commitment_cannot_roll_back() {
        let directory = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
        let authorization = ledger
            .append(input(
                "transition-side-effect",
                "agent:side-effect",
                TransitionRecordKind::Authorization,
                "auth-side-effect",
                TransitionLinks::default(),
            ))
            .expect("authorization");
        let mut observation = input(
            "transition-side-effect",
            "agent:side-effect",
            TransitionRecordKind::Observation,
            "obs-side-effect",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref.clone()),
                ..TransitionLinks::default()
            },
        );
        observation.side_effect_committed = Some(true);
        let observation = ledger.append(observation).expect("observation");

        let mut integrity = input(
            "transition-side-effect",
            "agent:side-effect",
            TransitionRecordKind::ResponseIntegrity,
            "integrity-side-effect",
            TransitionLinks {
                authorization_ref: Some(authorization.body.record_ref),
                observation_refs: vec![observation.body.record_ref],
                ..TransitionLinks::default()
            },
        );
        integrity.side_effect_committed = Some(false);
        let error = ledger.append(integrity).expect_err("rollback must fail");
        assert!(matches!(error, TransitionLedgerError::SideEffectRollback));
    }

    #[test]
    fn reauthorization_requires_explicit_supersession() {
        let directory = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
        let authorization = ledger
            .append(input(
                "transition-reauth",
                "agent:reauth",
                TransitionRecordKind::Authorization,
                "auth-old",
                TransitionLinks::default(),
            ))
            .expect("old authorization");
        let error = ledger
            .append(input(
                "transition-reauth",
                "agent:reauth",
                TransitionRecordKind::Authorization,
                "auth-new-invalid",
                TransitionLinks::default(),
            ))
            .expect_err("implicit reauthorization must fail");
        assert!(matches!(
            error,
            TransitionLedgerError::ReauthorizationWithoutSupersession
        ));

        let accepted = ledger
            .append(input(
                "transition-reauth",
                "agent:reauth",
                TransitionRecordKind::Authorization,
                "auth-new-valid",
                TransitionLinks {
                    authorization_ref: Some(authorization.body.record_ref),
                    ..TransitionLinks::default()
                },
            ))
            .expect("explicit supersession");
        assert_eq!(
            ledger
                .projection("transition-reauth")
                .expect("projection")
                .authorization_ref
                .as_deref(),
            Some(accepted.body.record_ref.as_str())
        );
        assert_eq!(
            ledger
                .projection("transition-reauth")
                .expect("projection")
                .authorization_epoch,
            2
        );
    }

    #[test]
    fn tampered_snapshot_digest_is_rejected() {
        let directory = tempdir().expect("tempdir");
        let snapshot_path = {
            let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
            ledger
                .append(input(
                    "transition-snapshot",
                    "agent:snapshot",
                    TransitionRecordKind::Authorization,
                    "auth-snapshot",
                    TransitionLinks::default(),
                ))
                .expect("authorization");
            ledger.write_snapshot(10).expect("snapshot").path
        };

        let bytes = fs::read(&snapshot_path).expect("read snapshot");
        let mut snapshot: TransitionLedgerSnapshot =
            serde_cbor::from_slice(&bytes).expect("decode snapshot");
        snapshot.snapshot_digest = reference("tampered-snapshot-digest");
        fs::write(
            &snapshot_path,
            serde_cbor::to_vec(&snapshot).expect("encode snapshot"),
        )
        .expect("write tampered snapshot");

        let error = TrustworthyTransitionLedger::open(directory.path())
            .err()
            .expect("tampered snapshot must fail");
        assert!(matches!(
            error,
            TransitionLedgerError::SnapshotDigestMismatch
        ));
    }

    #[test]
    fn semantic_event_hash_is_checked_beyond_wal_crc() {
        let directory = tempdir().expect("tempdir");
        let first = {
            let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
            ledger
                .append(input(
                    "transition-event-hash",
                    "agent:event-hash",
                    TransitionRecordKind::Authorization,
                    "auth-event-hash",
                    TransitionLinks::default(),
                ))
                .expect("authorization")
        };

        let body = TransitionEventBody {
            schema: EVENT_SCHEMA.to_owned(),
            profile: PROFILE.to_owned(),
            sequence: 2,
            transition_id: "transition-event-hash".to_owned(),
            subject_id: "agent:event-hash".to_owned(),
            kind: TransitionRecordKind::Observation,
            record_ref: reference("record:bad-event"),
            payload_digest: reference("payload:bad-event"),
            links: TransitionLinks {
                authorization_ref: Some(first.body.record_ref),
                ..TransitionLinks::default()
            },
            dimensions: None,
            side_effect_committed: None,
            captured_at_ms: 2,
            previous_event_hash: Some(first.event_hash),
        };
        let event = TransitionEvent {
            body,
            event_hash: reference("intentionally-wrong-event-hash"),
        };
        let mut raw_store = Store::open(directory.path()).expect("raw store");
        raw_store
            .append(&serde_cbor::to_vec(&event).expect("encode event"))
            .expect("append and sync bad semantic event");
        drop(raw_store);

        let error = TrustworthyTransitionLedger::open(directory.path())
            .err()
            .expect("bad event hash must fail");
        assert!(matches!(error, TransitionLedgerError::EventHashMismatch));
    }

    #[test]
    fn snapshot_tail_replay_matches_full_replay() {
        let directory = tempdir().expect("tempdir");
        {
            let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open");
            let authorization = ledger
                .append(input(
                    "transition-tail",
                    "agent:tail",
                    TransitionRecordKind::Authorization,
                    "auth-tail",
                    TransitionLinks::default(),
                ))
                .expect("authorization");
            ledger.write_snapshot(1).expect("snapshot");
            ledger
                .append(input(
                    "transition-tail",
                    "agent:tail",
                    TransitionRecordKind::Observation,
                    "obs-tail",
                    TransitionLinks {
                        authorization_ref: Some(authorization.body.record_ref),
                        ..TransitionLinks::default()
                    },
                ))
                .expect("tail observation");
        }

        let reopened = TrustworthyTransitionLedger::open(directory.path()).expect("reopen");
        assert_eq!(reopened.event_count(), 2);
        assert_eq!(
            reopened
                .projection("transition-tail")
                .expect("projection")
                .observation_refs
                .len(),
            1
        );
    }

    #[test]
    fn sha256_reference_validation_is_strict() {
        let mut seen = BTreeSet::new();
        let lower = reference("lowercase");
        assert!(seen.insert(lower.clone()));
        assert!(validate_ref(&lower, "test").is_ok());
        assert!(validate_ref("sha256:ABC", "test").is_err());
        assert!(validate_ref("not-a-digest", "test").is_err());
    }
}
