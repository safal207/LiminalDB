from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


root = Path(".")

transition_path = root / "liminal-db/crates/liminal-store/src/trustworthy_transition.rs"
transition = transition_path.read_text()
transition = replace_once(
    transition,
    '''pub struct TransitionLedgerSnapshotInfo {
    pub path: PathBuf,
    pub offset: Offset,
    pub event_count: u64,
    pub projection_count: usize,
    pub snapshot_digest: String,
}
''',
    '''pub struct TransitionLedgerSnapshotInfo {
    pub(crate) path: PathBuf,
    pub(crate) offset: Offset,
    pub(crate) event_count: u64,
    pub(crate) projection_count: usize,
    pub(crate) snapshot_digest: String,
    pub(crate) head_event_hash: Option<String>,
    pub(crate) projection_digest: String,
}

impl TransitionLedgerSnapshotInfo {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn offset(&self) -> Offset {
        self.offset
    }

    pub fn event_count(&self) -> u64 {
        self.event_count
    }

    pub fn projection_count(&self) -> usize {
        self.projection_count
    }

    pub fn snapshot_digest(&self) -> &str {
        &self.snapshot_digest
    }
}
''',
    "snapshot info capability",
)
transition = replace_once(
    transition,
    '''        Ok(TransitionLedgerSnapshotInfo {
            path: self.snapshot_path.clone(),
            offset: snapshot.body.wal_offset.into(),
            event_count: self.event_count(),
            projection_count: self.state.projections.len(),
            snapshot_digest,
        })
''',
    '''        Ok(TransitionLedgerSnapshotInfo {
            path: self.snapshot_path.clone(),
            offset: snapshot.body.wal_offset.into(),
            event_count: self.event_count(),
            projection_count: self.state.projections.len(),
            snapshot_digest,
            head_event_hash: snapshot.body.head_event_hash.clone(),
            projection_digest: snapshot.body.projection_digest.clone(),
        })
''',
    "snapshot info binding material",
)
transition_path.write_text(transition)

checkpoint_path = root / "liminal-db/crates/liminal-store/src/checkpoint.rs"
checkpoint = checkpoint_path.read_text()
checkpoint = replace_once(
    checkpoint,
    '''        validate_ref(&storage_root_identity, "storage_root_identity")?;
        validate_ref(&snapshot.snapshot_digest, "snapshot_digest")?;
        if snapshot.event_count != self.event_count()
            || snapshot.projection_count != self.projections().len()
        {
            return Err(CheckpointError::SnapshotStateMismatch);
        }
        let event_chain_head = self
            .head_event_hash()
            .ok_or(CheckpointError::MissingLedgerHead)?
            .to_owned();
        validate_ref(&event_chain_head, "event_chain_head")?;
        let projection_digest = digest_cbor(self.projections())?;
''',
    '''        validate_ref(&storage_root_identity, "storage_root_identity")?;
        validate_ref(&snapshot.snapshot_digest, "snapshot_digest")?;
        let event_chain_head = self
            .head_event_hash()
            .ok_or(CheckpointError::MissingLedgerHead)?
            .to_owned();
        validate_ref(&event_chain_head, "event_chain_head")?;
        let projection_digest = digest_cbor(self.projections())?;
        if snapshot.path.as_path() != self.snapshot_path()
            || snapshot.event_count != self.event_count()
            || snapshot.projection_count != self.projections().len()
            || snapshot.head_event_hash.as_deref() != Some(event_chain_head.as_str())
            || snapshot.projection_digest != projection_digest
        {
            return Err(CheckpointError::SnapshotStateMismatch);
        }
''',
    "checkpoint snapshot binding",
)
checkpoint_path.write_text(checkpoint)

test_path = root / "liminal-db/crates/liminal-store/tests/checkpoint_ledger_integration.rs"
test_path.write_text('''use liminal_store::{
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
''')

print("Applied guarded PR92 snapshot-binding fix")
