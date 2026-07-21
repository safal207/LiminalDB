#![cfg(feature = "durability-test-hooks")]

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

#[test]
fn append_failures_poison_until_reopen_and_replay() {
    let cases = [
        (AppendFailpoint::BeforeWrite, 0_u64),
        (AppendFailpoint::AfterWriteBeforeSync, 1_u64),
        (AppendFailpoint::AfterSyncBeforeAck, 1_u64),
    ];

    for (index, (failpoint, expected_recovered_events)) in cases.into_iter().enumerate() {
        let root = tempdir().expect("tempdir");
        let mut ledger = TrustworthyTransitionLedger::open(root.path()).expect("open");

        set_append_failpoint_for_test(failpoint);
        let first_error = ledger
            .append(authorization(&format!("first-{index}")))
            .expect_err("injected append must fail");
        assert!(matches!(first_error, TransitionLedgerError::Storage(_)));

        let poisoned_error = ledger
            .append(authorization(&format!("second-{index}")))
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
}
