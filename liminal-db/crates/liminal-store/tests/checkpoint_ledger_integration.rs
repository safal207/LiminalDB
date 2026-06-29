use liminal_store::{
    sha256_ref, CheckpointLedgerExt, TransitionEventInput, TransitionLinks,
    TransitionRecordKind, TrustworthyTransitionLedger,
};
use tempfile::tempdir;

#[test]
fn ledger_snapshot_exports_checkpoint_material() {
    let directory = tempdir().expect("tempdir");
    let mut ledger = TrustworthyTransitionLedger::open(directory.path()).expect("open ledger");
    ledger
        .append(TransitionEventInput {
            transition_id: "transition-checkpoint".to_owned(),
            subject_id: "agent:checkpoint".to_owned(),
            kind: TransitionRecordKind::Authorization,
            record_ref: sha256_ref(b"authorization-record"),
            payload_digest: sha256_ref(b"authorization-payload"),
            links: TransitionLinks::default(),
            dimensions: None,
            side_effect_committed: Some(false),
            captured_at_ms: 100,
        })
        .expect("append authorization");

    let snapshot = ledger.write_snapshot(110).expect("write snapshot");
    let material = ledger
        .checkpoint_material(sha256_ref(b"logical-storage-root"), &snapshot)
        .expect("checkpoint material");

    assert_eq!(material.last_sequence, 1);
    assert_eq!(material.snapshot_digest, snapshot.snapshot_digest);
    assert_eq!(material.wal_segment, snapshot.offset.segment);
    assert_eq!(material.wal_offset, snapshot.offset.position);
    assert_eq!(material.event_chain_head, ledger.head_event_hash().unwrap());
    assert_eq!(
        material.ledger_profile,
        "org.liminaldb.trustworthy-transition-ledger.v0.1"
    );
}
