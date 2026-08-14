#![cfg(feature = "durability-test-hooks")]

use std::fs::OpenOptions;
use std::io::Write;

use liminal_store::{
    set_append_failpoint_for_test, sha256_ref, AppendFailpoint, TransitionEventInput,
    TransitionLedgerError, TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};
use tempfile::tempdir;

fn authorization(label: &str) -> TransitionEventInput {
    TransitionEventInput {
        transition_id: format!("transition-{label}"),
        subject_id: "agent:fault-injection".to_owned(),
        kind: TransitionRecordKind::Authorization,
        record_ref: sha256_ref(format!("record:{label}").as_bytes()),
        payload_digest: sha256_ref(format!("payload:{label}").as_bytes()),
        links: TransitionLinks::default(),
        dimensions: None,
        side_effect_committed: None,
        captured_at_ms: 1,
    }
}

fn assert_failure_recovery(
    failpoint: AppendFailpoint,
    expected_recovered_events: u64,
    label: &str,
) {
    let root = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");

    set_append_failpoint_for_test(failpoint);
    let first_error = ledger
        .append(authorization(&format!("first-{label}")))
        .expect_err("injected append must fail");
    assert!(matches!(first_error, TransitionLedgerError::Storage(_)));

    let poisoned_error = ledger
        .append(authorization(&format!("second-{label}")))
        .expect_err("poisoned ledger must reject further append");
    assert!(matches!(
        poisoned_error,
        TransitionLedgerError::PoisonedAfterStorageFailure
    ));
    drop(ledger);

    let recovered = TrustworthyTransitionLedger::open(root.path()).expect("reopen and replay");
    assert_eq!(
        recovered.event_count(),
        expected_recovered_events,
        "unexpected replay result for {failpoint:?}"
    );
}

#[test]
fn before_write_failure_recovers_zero_events() {
    assert_failure_recovery(AppendFailpoint::BeforeWrite, 0, "before-write");
}

#[test]
fn length_only_tail_is_truncated() {
    assert_failure_recovery(AppendFailpoint::AfterLengthWrite, 0, "after-length");
}

#[test]
fn payload_without_crc_tail_is_truncated() {
    assert_failure_recovery(AppendFailpoint::AfterPayloadWrite, 0, "after-payload");
}

#[test]
fn complete_unsynced_frame_is_replayed_when_present() {
    // This is a non-crash error simulation, so the complete frame remains visible.
    // A real pre-sync crash may lose it; reopen always follows the bytes that survived.
    assert_failure_recovery(
        AppendFailpoint::AfterWriteBeforeSync,
        1,
        "after-write-before-sync",
    );
}

#[test]
fn synced_unacknowledged_frame_is_replayed() {
    assert_failure_recovery(
        AppendFailpoint::AfterSyncBeforeAck,
        1,
        "after-sync-before-ack",
    );
}

#[test]
fn append_failpoint_is_thread_local() {
    std::thread::spawn(|| {
        set_append_failpoint_for_test(AppendFailpoint::BeforeWrite);
    })
    .join()
    .expect("failpoint thread");

    let root = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
    ledger
        .append(authorization("unaffected-parent-thread"))
        .expect("another thread's failpoint must not leak");

    set_append_failpoint_for_test(AppendFailpoint::BeforeWrite);
    let error = ledger
        .append(authorization("same-thread-failure"))
        .expect_err("same-thread failpoint must fire");
    assert!(matches!(error, TransitionLedgerError::Storage(_)));
    drop(ledger);

    let recovered = TrustworthyTransitionLedger::open(root.path()).expect("reopen");
    assert_eq!(recovered.event_count(), 1);
}

#[test]
fn ambiguous_partial_payload_fails_closed_on_reopen() {
    let root = tempdir().expect("tempdir");
    {
        let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");
        ledger
            .append(authorization("durable-before-partial-tail"))
            .expect("durable event");
    }

    let wal_path = root.path().join("data/00000001.wal");
    let mut wal = OpenOptions::new()
        .append(true)
        .open(&wal_path)
        .expect("open WAL tail");
    wal.write_all(&32_u32.to_le_bytes())
        .expect("write declared payload length");
    wal.write_all(b"partial")
        .expect("write ambiguous payload prefix");
    wal.flush().expect("flush ambiguous tail");
    drop(wal);

    let error = TrustworthyTransitionLedger::open(root.path())
        .err()
        .expect("ambiguous partial payload must fail closed");
    assert!(matches!(error, TransitionLedgerError::Storage(_)));
}
