#![cfg(feature = "durability-test-hooks")]

use liminal_store::{
    set_append_failpoint_for_test, sha256_ref, AppendFailpoint, ProofPathAppendOutcome,
    ProofPathDurableError, ProofPathDurableInput, ProofPathDurableLedger,
};
use tempfile::tempdir;

const LOGICAL_OPERATION: &str = "crossmint-public-example-001";
const VALID_TIME_MS: u64 = 1_786_694_400_000;
const TRANSACTION_TIME_MS: u64 = 1_786_694_460_000;

fn input(transaction_time_ms: u64) -> ProofPathDurableInput {
    ProofPathDurableInput {
        logical_operation_id: LOGICAL_OPERATION.to_owned(),
        source_event_bytes: b"accepted-proofpath-event\n".to_vec(),
        admission_report_bytes: b"canonical-liminaldb-admission-report\n".to_vec(),
        source_receipt_ref: sha256_ref(b"native-proofpath-receipt"),
        valid_time_ms: VALID_TIME_MS,
        transaction_time_ms,
        storage_admission_ref: sha256_ref(b"system-005-local-test-storage-admission"),
    }
}

#[test]
fn synced_but_unacknowledged_append_recovers_and_retry_deduplicates() {
    let root = tempdir().expect("tempdir");
    let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");

    set_append_failpoint_for_test(AppendFailpoint::AfterSyncBeforeAck);
    let error = ledger
        .append(input(TRANSACTION_TIME_MS))
        .expect_err("ack-path failure must surface");
    assert!(matches!(error, ProofPathDurableError::Storage(_)));

    let poisoned = ledger
        .append(input(TRANSACTION_TIME_MS + 1))
        .expect_err("ambiguous writer must be poisoned until reopen");
    assert!(matches!(
        poisoned,
        ProofPathDurableError::PoisonedAfterStorageFailure
    ));
    drop(ledger);

    let mut recovered = ProofPathDurableLedger::open(root.path(), "system-005").expect("reopen");
    assert_eq!(recovered.event_count(), 1);
    let record = recovered
        .get(LOGICAL_OPERATION)
        .expect("synced record must replay");
    assert_eq!(record.body.transaction_time_ms, TRANSACTION_TIME_MS);

    let retry = recovered
        .append(input(TRANSACTION_TIME_MS + 60_000))
        .expect("retry after replay must be idempotent");
    assert!(matches!(retry, ProofPathAppendOutcome::AlreadyPresent(_)));
    assert_eq!(recovered.event_count(), 1);
    assert_eq!(retry.record().body.transaction_time_ms, TRANSACTION_TIME_MS);
}

#[test]
fn prewrite_failure_recovers_no_record() {
    let root = tempdir().expect("tempdir");
    let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
    set_append_failpoint_for_test(AppendFailpoint::BeforeWrite);
    let error = ledger
        .append(input(TRANSACTION_TIME_MS))
        .expect_err("prewrite failure");
    assert!(matches!(error, ProofPathDurableError::Storage(_)));
    drop(ledger);

    let reopened = ProofPathDurableLedger::open(root.path(), "system-005").expect("reopen");
    assert_eq!(reopened.event_count(), 0);
    assert!(reopened.get(LOGICAL_OPERATION).is_none());
}

#[test]
fn torn_payload_tail_is_truncated_before_replay() {
    let root = tempdir().expect("tempdir");
    let mut ledger = ProofPathDurableLedger::open(root.path(), "system-005").expect("open");
    set_append_failpoint_for_test(AppendFailpoint::AfterPayloadWrite);
    let error = ledger
        .append(input(TRANSACTION_TIME_MS))
        .expect_err("payload-only frame must fail");
    assert!(matches!(error, ProofPathDurableError::Storage(_)));
    drop(ledger);

    let reopened = ProofPathDurableLedger::open(root.path(), "system-005").expect("reopen");
    assert_eq!(reopened.event_count(), 0);
}
