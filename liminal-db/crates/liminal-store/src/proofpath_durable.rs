use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::wal::{Offset, Store};

pub const PROOFPATH_DURABLE_RECORD_SCHEMA: &str = "liminaldb.proofpath-durable-record.v0.1";
pub const PROOFPATH_DURABLE_PROFILE: &str = "org.liminaldb.proofpath-durable-ledger.v0.1";
pub const PROOFPATH_PRODUCER_REPOSITORY: &str = "safal207/ProofPath";
pub const PROOFPATH_CAPABILITY_ID: &str = "proofpath.scig.v0.1";
pub const PROOFPATH_CAPABILITY_COMMIT: &str = "685d50e256a5125a21f4c4584b326411caaa64ad";
pub const LIMINALDB_CONSUMER_REPOSITORY: &str = "safal207/LiminalDB";
pub const LIMINALDB_PROOFPATH_IMPORT_COMMIT: &str = "00580ff097dee61b45ad3c8a3c36ae5f548f572d";
pub const LIMINALDB_AUDIT_EVENT_CONTRACT_BLOB: &str = "fd733971aaae089df770062bcf7f2c2d6d19ca1d";
pub const PROOFPATH_PERSISTENCE_SCOPE: &str = "local_test_only";
const RECORD_KIND: &str = "proofpath.scig.verification.observed";
const LEDGER_DIR: &str = "proofpath-durable-v0.1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofPathDurableInput {
    pub logical_operation_id: String,
    pub source_event_bytes: Vec<u8>,
    pub admission_report_bytes: Vec<u8>,
    pub source_receipt_ref: String,
    pub valid_time_ms: u64,
    pub transaction_time_ms: u64,
    pub storage_admission_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofPathDurableRecordBody {
    pub schema: String,
    pub profile: String,
    pub sequence: u64,
    pub namespace: String,
    pub ingestion_key: String,
    pub logical_operation_id: String,
    pub source_event_sha256: String,
    pub source_receipt_ref: String,
    pub admission_report_sha256: String,
    pub producer_repository: String,
    pub producer_capability_id: String,
    pub producer_capability_commit: String,
    pub consumer_repository: String,
    pub consumer_import_commit: String,
    pub consumer_contract_blob_sha: String,
    pub valid_time_ms: u64,
    pub transaction_time_ms: u64,
    pub source_event_bytes: Vec<u8>,
    pub admission_report_bytes: Vec<u8>,
    pub storage_admission_ref: String,
    pub persistence_scope: String,
    pub storage_write_authorized: bool,
    pub execution_authorized: bool,
    pub mutation_authorized: bool,
    pub external_effects_authorized: bool,
    pub previous_record_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofPathDurableRecord {
    pub body: ProofPathDurableRecordBody,
    pub record_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofPathAppendOutcome {
    Inserted(ProofPathDurableRecord),
    AlreadyPresent(ProofPathDurableRecord),
}

impl ProofPathAppendOutcome {
    pub fn record(&self) -> &ProofPathDurableRecord {
        match self {
            Self::Inserted(record) | Self::AlreadyPresent(record) => record,
        }
    }

    pub fn inserted(&self) -> bool {
        matches!(self, Self::Inserted(_))
    }
}

#[derive(Debug, Error)]
pub enum ProofPathDurableError {
    #[error("storage error: {0}")]
    Storage(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("invalid namespace")]
    InvalidNamespace,
    #[error("invalid logical_operation_id")]
    InvalidLogicalOperationId,
    #[error("source event bytes must be non-empty")]
    EmptySourceEvent,
    #[error("admission report bytes must be non-empty")]
    EmptyAdmissionReport,
    #[error("invalid sha256 reference in {0}")]
    InvalidReference(&'static str),
    #[error("invalid temporal order: transaction_time_ms must be >= valid_time_ms")]
    InvalidTemporalOrder,
    #[error("record schema or profile mismatch")]
    RecordProfileMismatch,
    #[error("record namespace mismatch")]
    NamespaceMismatch,
    #[error("record sequence mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },
    #[error("record previous hash does not match ledger head")]
    PreviousRecordHashMismatch,
    #[error("record hash mismatch")]
    RecordHashMismatch,
    #[error("record ingestion key mismatch")]
    IngestionKeyMismatch,
    #[error("source event digest mismatch")]
    SourceEventDigestMismatch,
    #[error("admission report digest mismatch")]
    AdmissionReportDigestMismatch,
    #[error("unsupported producer capability identity")]
    ProducerCapabilityMismatch,
    #[error("unsupported consumer contract identity")]
    ConsumerContractMismatch,
    #[error("durable record violates the local-test authority boundary")]
    AuthorityBoundaryViolation,
    #[error("duplicate ingestion key found during replay: {0}")]
    DuplicateIngestionKey(String),
    #[error("same durable ingestion key was reused for different semantic evidence")]
    IdempotencyConflict,
    #[error(
        "ledger is poisoned after an ambiguous storage failure; reopen and replay before appending"
    )]
    PoisonedAfterStorageFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProofPathDurableState {
    next_sequence: u64,
    head_record_hash: Option<String>,
    records: BTreeMap<String, ProofPathDurableRecord>,
}

impl Default for ProofPathDurableState {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            head_record_hash: None,
            records: BTreeMap::new(),
        }
    }
}

pub struct ProofPathDurableLedger {
    namespace: String,
    store: Store,
    state: ProofPathDurableState,
    poisoned: bool,
}

impl ProofPathDurableLedger {
    /// Opens one physically isolated local/test ProofPath persistence namespace.
    ///
    /// The returned ledger replays the full WAL from offset zero before accepting
    /// new writes. Opening the ledger does not grant execution or mutation authority.
    pub fn open<P: AsRef<Path>>(
        root: P,
        namespace: impl Into<String>,
    ) -> Result<Self, ProofPathDurableError> {
        let namespace = normalize_namespace(namespace.into())?;
        let path = root.as_ref().join(LEDGER_DIR).join(&namespace);
        let store = Store::open(path).map_err(storage_error)?;
        let state = replay_from(&store, &namespace)?;
        Ok(Self {
            namespace,
            store,
            state,
            poisoned: false,
        })
    }

    /// Appends exact accepted ProofPath evidence after a separate local/test
    /// storage-admission reference has been supplied.
    ///
    /// The idempotency key is derived from namespace + logical operation + record
    /// kind, not from payload bytes. A retry of the same semantic evidence returns
    /// `AlreadyPresent`; changed evidence under the same operation fails closed.
    pub fn append(
        &mut self,
        input: ProofPathDurableInput,
    ) -> Result<ProofPathAppendOutcome, ProofPathDurableError> {
        if self.poisoned {
            return Err(ProofPathDurableError::PoisonedAfterStorageFailure);
        }
        let input = normalize_input(input)?;
        let ingestion_key = ingestion_key(&self.namespace, &input.logical_operation_id);

        if let Some(existing) = self.state.records.get(&ingestion_key) {
            if equivalent_semantic_input(existing, &input) {
                return Ok(ProofPathAppendOutcome::AlreadyPresent(existing.clone()));
            }
            return Err(ProofPathDurableError::IdempotencyConflict);
        }

        let body = ProofPathDurableRecordBody {
            schema: PROOFPATH_DURABLE_RECORD_SCHEMA.to_owned(),
            profile: PROOFPATH_DURABLE_PROFILE.to_owned(),
            sequence: self.state.next_sequence,
            namespace: self.namespace.clone(),
            ingestion_key,
            logical_operation_id: input.logical_operation_id,
            source_event_sha256: sha256_ref(&input.source_event_bytes),
            source_receipt_ref: input.source_receipt_ref,
            admission_report_sha256: sha256_ref(&input.admission_report_bytes),
            producer_repository: PROOFPATH_PRODUCER_REPOSITORY.to_owned(),
            producer_capability_id: PROOFPATH_CAPABILITY_ID.to_owned(),
            producer_capability_commit: PROOFPATH_CAPABILITY_COMMIT.to_owned(),
            consumer_repository: LIMINALDB_CONSUMER_REPOSITORY.to_owned(),
            consumer_import_commit: LIMINALDB_PROOFPATH_IMPORT_COMMIT.to_owned(),
            consumer_contract_blob_sha: LIMINALDB_AUDIT_EVENT_CONTRACT_BLOB.to_owned(),
            valid_time_ms: input.valid_time_ms,
            transaction_time_ms: input.transaction_time_ms,
            source_event_bytes: input.source_event_bytes,
            admission_report_bytes: input.admission_report_bytes,
            storage_admission_ref: input.storage_admission_ref,
            persistence_scope: PROOFPATH_PERSISTENCE_SCOPE.to_owned(),
            storage_write_authorized: true,
            execution_authorized: false,
            mutation_authorized: false,
            external_effects_authorized: false,
            previous_record_hash: self.state.head_record_hash.clone(),
        };
        let record = ProofPathDurableRecord {
            record_hash: digest_cbor(&body)?,
            body,
        };

        let mut candidate = self.state.clone();
        apply_record(&mut candidate, &self.namespace, &record)?;
        let bytes = serde_cbor::to_vec(&record).map_err(encoding_error)?;
        if let Err(error) = self.store.append(&bytes) {
            self.poisoned = true;
            return Err(storage_error(error));
        }
        self.state = candidate;
        Ok(ProofPathAppendOutcome::Inserted(record))
    }

    pub fn get(&self, logical_operation_id: &str) -> Option<&ProofPathDurableRecord> {
        let logical_operation_id = logical_operation_id.trim();
        if logical_operation_id.is_empty() {
            return None;
        }
        let key = ingestion_key(&self.namespace, logical_operation_id);
        self.state.records.get(&key)
    }

    pub fn event_count(&self) -> u64 {
        self.state.next_sequence.saturating_sub(1)
    }

    pub fn head_record_hash(&self) -> Option<&str> {
        self.state.head_record_hash.as_deref()
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
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

fn normalize_namespace(value: String) -> Result<String, ProofPathDurableError> {
    let trimmed = value.trim();
    let valid_len = !trimmed.is_empty() && trimmed.len() <= 64;
    let valid_chars = trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
    if !valid_len
        || trimmed != value
        || trimmed == "."
        || trimmed == ".."
        || !valid_chars
        || !trimmed.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(ProofPathDurableError::InvalidNamespace);
    }
    Ok(value)
}

fn normalize_input(
    mut input: ProofPathDurableInput,
) -> Result<ProofPathDurableInput, ProofPathDurableError> {
    let logical = input.logical_operation_id.trim();
    if logical.is_empty() || logical != input.logical_operation_id {
        return Err(ProofPathDurableError::InvalidLogicalOperationId);
    }
    if input.source_event_bytes.is_empty() {
        return Err(ProofPathDurableError::EmptySourceEvent);
    }
    if input.admission_report_bytes.is_empty() {
        return Err(ProofPathDurableError::EmptyAdmissionReport);
    }
    validate_sha256_ref(&input.source_receipt_ref, "source_receipt_ref")?;
    validate_sha256_ref(&input.storage_admission_ref, "storage_admission_ref")?;
    if input.valid_time_ms == 0 || input.transaction_time_ms < input.valid_time_ms {
        return Err(ProofPathDurableError::InvalidTemporalOrder);
    }
    input.logical_operation_id = logical.to_owned();
    Ok(input)
}

fn replay_from(
    store: &Store,
    namespace: &str,
) -> Result<ProofPathDurableState, ProofPathDurableError> {
    let stream = store.stream_from(Offset::start()).map_err(storage_error)?;
    let mut state = ProofPathDurableState::default();
    for record in stream {
        let bytes = record.map_err(storage_error)?;
        let decoded: ProofPathDurableRecord =
            serde_cbor::from_slice(&bytes).map_err(encoding_error)?;
        apply_record(&mut state, namespace, &decoded)?;
    }
    Ok(state)
}

fn apply_record(
    state: &mut ProofPathDurableState,
    namespace: &str,
    record: &ProofPathDurableRecord,
) -> Result<(), ProofPathDurableError> {
    let body = &record.body;
    if body.schema != PROOFPATH_DURABLE_RECORD_SCHEMA || body.profile != PROOFPATH_DURABLE_PROFILE {
        return Err(ProofPathDurableError::RecordProfileMismatch);
    }
    if body.namespace != namespace {
        return Err(ProofPathDurableError::NamespaceMismatch);
    }
    if body.sequence != state.next_sequence {
        return Err(ProofPathDurableError::SequenceMismatch {
            expected: state.next_sequence,
            actual: body.sequence,
        });
    }
    if body.previous_record_hash != state.head_record_hash {
        return Err(ProofPathDurableError::PreviousRecordHashMismatch);
    }
    if digest_cbor(body)? != record.record_hash {
        return Err(ProofPathDurableError::RecordHashMismatch);
    }
    validate_sha256_ref(&record.record_hash, "record_hash")?;
    validate_sha256_ref(&body.source_receipt_ref, "source_receipt_ref")?;
    validate_sha256_ref(&body.storage_admission_ref, "storage_admission_ref")?;
    if body.logical_operation_id.trim().is_empty()
        || body.logical_operation_id.trim() != body.logical_operation_id
    {
        return Err(ProofPathDurableError::InvalidLogicalOperationId);
    }
    if body.valid_time_ms == 0 || body.transaction_time_ms < body.valid_time_ms {
        return Err(ProofPathDurableError::InvalidTemporalOrder);
    }
    let expected_key = ingestion_key(namespace, &body.logical_operation_id);
    if body.ingestion_key != expected_key {
        return Err(ProofPathDurableError::IngestionKeyMismatch);
    }
    validate_sha256_ref(&body.ingestion_key, "ingestion_key")?;
    if body.source_event_sha256 != sha256_ref(&body.source_event_bytes) {
        return Err(ProofPathDurableError::SourceEventDigestMismatch);
    }
    if body.admission_report_sha256 != sha256_ref(&body.admission_report_bytes) {
        return Err(ProofPathDurableError::AdmissionReportDigestMismatch);
    }
    if body.producer_repository != PROOFPATH_PRODUCER_REPOSITORY
        || body.producer_capability_id != PROOFPATH_CAPABILITY_ID
        || body.producer_capability_commit != PROOFPATH_CAPABILITY_COMMIT
    {
        return Err(ProofPathDurableError::ProducerCapabilityMismatch);
    }
    if body.consumer_repository != LIMINALDB_CONSUMER_REPOSITORY
        || body.consumer_import_commit != LIMINALDB_PROOFPATH_IMPORT_COMMIT
        || body.consumer_contract_blob_sha != LIMINALDB_AUDIT_EVENT_CONTRACT_BLOB
    {
        return Err(ProofPathDurableError::ConsumerContractMismatch);
    }
    if body.persistence_scope != PROOFPATH_PERSISTENCE_SCOPE
        || !body.storage_write_authorized
        || body.execution_authorized
        || body.mutation_authorized
        || body.external_effects_authorized
    {
        return Err(ProofPathDurableError::AuthorityBoundaryViolation);
    }
    if state.records.contains_key(&body.ingestion_key) {
        return Err(ProofPathDurableError::DuplicateIngestionKey(
            body.ingestion_key.clone(),
        ));
    }

    state
        .records
        .insert(body.ingestion_key.clone(), record.clone());
    state.head_record_hash = Some(record.record_hash.clone());
    state.next_sequence += 1;
    Ok(())
}

fn equivalent_semantic_input(
    existing: &ProofPathDurableRecord,
    input: &ProofPathDurableInput,
) -> bool {
    existing.body.logical_operation_id == input.logical_operation_id
        && existing.body.source_event_bytes == input.source_event_bytes
        && existing.body.admission_report_bytes == input.admission_report_bytes
        && existing.body.source_receipt_ref == input.source_receipt_ref
        && existing.body.valid_time_ms == input.valid_time_ms
        && existing.body.source_event_sha256 == sha256_ref(&input.source_event_bytes)
        && existing.body.admission_report_sha256 == sha256_ref(&input.admission_report_bytes)
}

fn ingestion_key(namespace: &str, logical_operation_id: &str) -> String {
    let seed = format!("{namespace}\0{logical_operation_id}\0{RECORD_KIND}");
    sha256_ref(seed.as_bytes())
}

fn validate_sha256_ref(reference: &str, label: &'static str) -> Result<(), ProofPathDurableError> {
    let bytes = reference.as_bytes();
    let valid = bytes.len() == 71
        && reference.starts_with("sha256:")
        && bytes[7..]
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    if !valid {
        return Err(ProofPathDurableError::InvalidReference(label));
    }
    Ok(())
}

fn digest_cbor<T: Serialize>(value: &T) -> Result<String, ProofPathDurableError> {
    let bytes = serde_cbor::to_vec(value).map_err(encoding_error)?;
    Ok(sha256_ref(&bytes))
}

fn storage_error(error: impl std::fmt::Display) -> ProofPathDurableError {
    ProofPathDurableError::Storage(error.to_string())
}

fn encoding_error(error: impl std::fmt::Display) -> ProofPathDurableError {
    ProofPathDurableError::Encoding(error.to_string())
}
