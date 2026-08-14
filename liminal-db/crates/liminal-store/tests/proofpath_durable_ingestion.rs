use std::fs;

use liminal_store::{
    sha256_ref, ProofPathAppendOutcome, ProofPathDurableError, ProofPathDurableInput,
    ProofPathDurableLedger, ProofPathDurableRecord, LIMINALDB_AUDIT_EVENT_CONTRACT_BLOB,
    LIMINALDB_PROOFPATH_IMPORT_COMMIT, PROOFPATH_CAPABILITY_COMMIT, PROOFPATH_PERSISTENCE_SCOPE,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

const LOGICAL_OPERATION: &str = "crossmint-public-example-001";
const VALID_TIME_MS: u64 = 1_786_694_400_000;
const TRANSACTION_TIME_MS: u64 = 1_786_694_460_000;

fn input(event: &[u8], admission: &[u8], transaction_time_ms: u64) -> ProofPathDurableInput {
    ProofPathDurableInput {
        logical_operation_id: LOGICAL_OPERATION.to_owned(),
        source_event_bytes: event.to_vec(),
        admission_report_bytes: admission.to_vec(),
        source_receipt_ref: sha256_ref(b"native-proofpath-receipt"),
        valid_time_ms: VALID_TIME_MS,
        transaction_time_ms,
        storage_admission_ref: sha256_ref(b"system-005-local-test-storage-admission"),
    }
}

fn source_event() -> &'static [u8] {
    br#"{"actor":"proofpath-scig-native-verifier","action":"proofpath.scig.verification.observed","correlationId":"crossmint-public-example-001"}
"#
}

fn admission_report() -> &'static [u8] {
    br#"{"mode":"dry_run","write_performed":false,"authority":{"durable_memory_accepted":false}}
"#
}

fn record_hash(record: &ProofPathDurableRecord) -> String {
    let bytes = serde_cbor::to_vec(&record.body).expect("serialize body");
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

#[test]
fn exact_artifact_survives_restart_byte_for_byte() {
    let root = tempdir().expect("tempdir");
    let inserted = {
        let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
        let outcome = ledger
            .append(input(
                source_event(),
                admission_report(),
                TRANSACTION_TIME_MS,
            ))
            .expect("append");
        assert!(outcome.inserted());
        assert_eq!(ledger.event_count(), 1);
        outcome.record().clone()
    };

    let reopened = ProofPathDurableLedger::open(root.path(), "system-005").expect("reopen");
    let recovered = reopened.get(LOGICAL_OPERATION).expect("recovered record");
    assert_eq!(recovered, &inserted);
    assert_eq!(recovered.body.source_event_bytes, source_event());
    assert_eq!(recovered.body.admission_report_bytes, admission_report());
    assert_eq!(recovered.body.valid_time_ms, VALID_TIME_MS);
    assert_eq!(recovered.body.transaction_time_ms, TRANSACTION_TIME_MS);
    assert_eq!(
        recovered.body.producer_capability_commit,
        PROOFPATH_CAPABILITY_COMMIT
    );
    assert_eq!(
        recovered.body.consumer_import_commit,
        LIMINALDB_PROOFPATH_IMPORT_COMMIT
    );
    assert_eq!(
        recovered.body.consumer_contract_blob_sha,
        LIMINALDB_AUDIT_EVENT_CONTRACT_BLOB
    );
    assert_eq!(
        recovered.body.persistence_scope,
        PROOFPATH_PERSISTENCE_SCOPE
    );
    assert!(recovered.body.storage_write_authorized);
    assert!(!recovered.body.execution_authorized);
    assert!(!recovered.body.mutation_authorized);
    assert!(!recovered.body.external_effects_authorized);
}

#[test]
fn same_operation_and_same_artifact_is_idempotent_after_restart() {
    let root = tempdir().expect("tempdir");
    {
        let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
        let first = ledger
            .append(input(
                source_event(),
                admission_report(),
                TRANSACTION_TIME_MS,
            ))
            .expect("first append");
        assert!(matches!(first, ProofPathAppendOutcome::Inserted(_)));
    }

    let mut reopened = ProofPathDurableLedger::open(root.path(), "system-005").expect("reopen");
    let mut retry = input(
        source_event(),
        admission_report(),
        TRANSACTION_TIME_MS + 60_000,
    );
    retry.storage_admission_ref = sha256_ref(b"system-005-retry-storage-admission");
    let outcome = reopened.append(retry).expect("idempotent retry");
    assert!(matches!(outcome, ProofPathAppendOutcome::AlreadyPresent(_)));
    assert_eq!(reopened.event_count(), 1);
    assert_eq!(
        outcome.record().body.transaction_time_ms,
        TRANSACTION_TIME_MS,
        "retry must preserve first durable transaction time"
    );
}

#[test]
fn same_operation_with_changed_artifact_fails_closed() {
    let root = tempdir().expect("tempdir");
    let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
    ledger
        .append(input(
            source_event(),
            admission_report(),
            TRANSACTION_TIME_MS,
        ))
        .expect("first append");

    let error = ledger
        .append(input(
            b"changed-proofpath-event\n",
            admission_report(),
            TRANSACTION_TIME_MS + 60_000,
        ))
        .expect_err("changed evidence under same operation must conflict");
    assert!(matches!(error, ProofPathDurableError::IdempotencyConflict));
    assert_eq!(ledger.event_count(), 1);
}

#[test]
fn namespaces_are_physically_and_semantically_isolated() {
    let root = tempdir().expect("tempdir");
    {
        let mut alpha = ProofPathDurableLedger::open(root.path(), "tenant-alpha").expect("alpha");
        alpha
            .append(input(
                source_event(),
                admission_report(),
                TRANSACTION_TIME_MS,
            ))
            .expect("alpha append");
    }
    {
        let mut beta = ProofPathDurableLedger::open(root.path(), "tenant-beta").expect("beta");
        beta.append(input(
            b"beta-specific-event\n",
            admission_report(),
            TRANSACTION_TIME_MS,
        ))
        .expect("beta append");
    }

    let alpha = ProofPathDurableLedger::open(root.path(), "tenant-alpha").expect("alpha reopen");
    let beta = ProofPathDurableLedger::open(root.path(), "tenant-beta").expect("beta reopen");
    assert_eq!(alpha.event_count(), 1);
    assert_eq!(beta.event_count(), 1);
    assert_eq!(
        alpha
            .get(LOGICAL_OPERATION)
            .unwrap()
            .body
            .source_event_bytes,
        source_event()
    );
    assert_eq!(
        beta.get(LOGICAL_OPERATION).unwrap().body.source_event_bytes,
        b"beta-specific-event\n"
    );
    assert!(root
        .path()
        .join("proofpath-durable-v0.1/tenant-alpha/data/00000001.wal")
        .is_file());
    assert!(root
        .path()
        .join("proofpath-durable-v0.1/tenant-beta/data/00000001.wal")
        .is_file());
}

#[test]
fn invalid_bitemporal_order_is_rejected_before_write() {
    let root = tempdir().expect("tempdir");
    let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
    let error = ledger
        .append(input(source_event(), admission_report(), VALID_TIME_MS - 1))
        .expect_err("transaction time before valid time must fail");
    assert!(matches!(error, ProofPathDurableError::InvalidTemporalOrder));
    assert_eq!(ledger.event_count(), 0);
}

#[test]
fn replay_rejects_authority_escalation_even_with_valid_hashes() {
    let root = tempdir().expect("tempdir");
    let first = {
        let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
        ledger
            .append(input(
                source_event(),
                admission_report(),
                TRANSACTION_TIME_MS,
            ))
            .expect("append")
            .record()
            .clone()
    };

    let ledger_root = root.path().join("proofpath-durable-v0.1/system-005");
    let mut malicious = first.clone();
    malicious.body.sequence = 2;
    malicious.body.previous_record_hash = Some(first.record_hash.clone());
    malicious.body.execution_authorized = true;
    malicious.record_hash = record_hash(&malicious);

    {
        let mut raw = liminal_store::Store::open(&ledger_root).expect("open raw store");
        let bytes = serde_cbor::to_vec(&malicious).expect("encode malicious record");
        raw.append(&bytes)
            .expect("append structurally valid malicious record");
    }

    let error = ProofPathDurableLedger::open(root.path(), "system-005")
        .err()
        .expect("replay must reject authority escalation");
    assert!(matches!(
        error,
        ProofPathDurableError::AuthorityBoundaryViolation
    ));

    fs::remove_dir_all(root.path()).ok();
}
