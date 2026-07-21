use liminal_store::{
    sha256_ref, CheckpointError, CheckpointLedgerExt, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustworthyTransitionLedger,
};
use tempfile::tempdir;

fn append_authorization(ledger: &mut TrustworthyTransitionLedger, label: &str) {
    ledger
        .append(TransitionEventInput {
            transition_id: format!("transition-{label}"),
            subject_id: "agent:checkpoint".to_owned(),
            kind: TransitionRecordKind::Authorization,
            record_ref: sha256_ref(format!("authorization-record-{label}").as_bytes()),
            payload_digest: sha256_ref(format!("authorization-payload-{label}").as_bytes()),
            links: TransitionLinks::default(),
            dimensions: None,
            side_effect_committed: Some(false),
            captured_at_ms: 100,
        })
        .expect("append authorization");
}

#[test]
fn ledger_snapshot_exports_checkpoint_material() {
    let directory = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open ledger");
    append_authorization(&mut ledger, "checkpoint");

    let snapshot = ledger.write_snapshot(110).expect("write snapshot");
    let material = ledger
        .checkpoint_material(sha256_ref(b"logical-storage-root"), &snapshot)
        .expect("checkpoint material");

    assert_eq!(material.last_sequence, 1);
    assert_eq!(material.snapshot_digest, snapshot.snapshot_digest());
    assert_eq!(material.wal_segment, snapshot.offset().segment);
    assert_eq!(material.wal_offset, snapshot.offset().position);
    assert_eq!(material.event_chain_head, ledger.head_event_hash().unwrap());
    assert_eq!(snapshot.event_count(), 1);
    assert_eq!(snapshot.projection_count(), 1);
    assert_eq!(snapshot.path(), ledger.snapshot_path());
    assert_eq!(
        material.ledger_profile,
        "org.liminaldb.trustworthy-transition-ledger.v0.1"
    );
}

#[test]
fn checkpoint_material_rejects_foreign_snapshot_with_matching_counts() {
    let first_directory = tempdir().expect("first tempdir");
    let second_directory = tempdir().expect("second tempdir");
    let mut first =
        TrustworthyTransitionLedger::open(first_directory.path()).expect("open first ledger");
    let mut second =
        TrustworthyTransitionLedger::open(second_directory.path()).expect("open second ledger");

    append_authorization(&mut first, "first");
    append_authorization(&mut second, "second");
    let foreign_snapshot = first.write_snapshot(110).expect("write foreign snapshot");

    let error = second
        .checkpoint_material(sha256_ref(b"logical-storage-root"), &foreign_snapshot)
        .expect_err("foreign snapshot must not bind to another ledger");
    assert_eq!(error, CheckpointError::SnapshotStateMismatch);
}

#[test]
fn checkpoint_material_rejects_stale_snapshot_after_ledger_advances() {
    let directory = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open ledger");
    append_authorization(&mut ledger, "initial");
    let stale_snapshot = ledger.write_snapshot(110).expect("write snapshot");

    ledger
        .append(TransitionEventInput {
            transition_id: "transition-initial".to_owned(),
            subject_id: "agent:checkpoint".to_owned(),
            kind: TransitionRecordKind::Observation,
            record_ref: sha256_ref(b"observation-record"),
            payload_digest: sha256_ref(b"observation-payload"),
            links: TransitionLinks {
                authorization_ref: Some(sha256_ref(b"authorization-record-initial")),
                ..TransitionLinks::default()
            },
            dimensions: None,
            side_effect_committed: None,
            captured_at_ms: 120,
        })
        .expect("append observation");

    let error = ledger
        .checkpoint_material(sha256_ref(b"logical-storage-root"), &stale_snapshot)
        .expect_err("stale snapshot must not bind after ledger advances");
    assert_eq!(error, CheckpointError::SnapshotStateMismatch);
}
