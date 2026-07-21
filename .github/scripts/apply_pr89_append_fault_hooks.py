from pathlib import Path


def insert_before_once(text: str, marker: str, addition: str, label: str) -> str:
    if addition.strip() in text:
        return text
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{label}: expected one marker, found {count}")
    return text.replace(marker, addition + marker, 1)


def insert_after_once(text: str, marker: str, addition: str, label: str) -> str:
    if addition.strip() in text:
        return text
    count = text.count(marker)
    if count != 1:
        raise SystemExit(f"{label}: expected one marker, found {count}")
    return text.replace(marker, marker + addition, 1)


def replace_method(text: str, start_marker: str, end_marker: str, replacement: str, label: str) -> str:
    start = text.find(start_marker)
    if start < 0:
        raise SystemExit(f"{label}: start marker not found")
    if text.find(start_marker, start + 1) >= 0:
        raise SystemExit(f"{label}: start marker is not unique")
    end = text.find(end_marker, start)
    if end < 0:
        raise SystemExit(f"{label}: end marker not found")
    current = text[start:end]
    required = ("self.writer.append(bytes)?", "self.writer.sync_all()?")
    missing = [token for token in required if token not in current]
    if missing:
        raise SystemExit(f"{label}: unexpected current method; missing {missing}")
    return text[:start] + replacement + text[end:]


root = Path(".")

cargo_path = root / "liminal-db/crates/liminal-store/Cargo.toml"
cargo = cargo_path.read_text()
cargo = insert_before_once(
    cargo,
    "[dependencies]\n",
    "[features]\ndurability-test-hooks = []\n\n",
    "Cargo feature insertion",
)
cargo_path.write_text(cargo)

lib_path = root / "liminal-db/crates/liminal-store/src/lib.rs"
lib = lib_path.read_text()
lib = insert_before_once(
    lib,
    "pub use wal::{Offset, Store, WalStream};\n",
    '#[cfg(feature = "durability-test-hooks")]\n'
    "pub use wal::{set_append_failpoint_for_test, AppendFailpoint};\n",
    "fault-hook export insertion",
)
lib_path.write_text(lib)

wal_path = root / "liminal-db/crates/liminal-store/src/wal.rs"
wal = wal_path.read_text()
wal = insert_after_once(
    wal,
    "use std::path::{Path, PathBuf};\n",
    '#[cfg(any(test, feature = "durability-test-hooks"))]\n'
    "use std::sync::atomic::{AtomicU8, Ordering};\n",
    "atomic import insertion",
)
wal = insert_after_once(
    wal,
    'const WRITER_LOCK_FILE: &str = ".liminaldb-writer.lock";\n',
    "\n"
    '#[cfg(any(test, feature = "durability-test-hooks"))]\n'
    "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n"
    "#[repr(u8)]\n"
    "pub enum AppendFailpoint {\n"
    "    Disabled = 0,\n"
    "    BeforeWrite = 1,\n"
    "    AfterWriteBeforeSync = 2,\n"
    "    AfterSyncBeforeAck = 3,\n"
    "}\n\n"
    '#[cfg(any(test, feature = "durability-test-hooks"))]\n'
    "static APPEND_FAILPOINT: AtomicU8 = AtomicU8::new(AppendFailpoint::Disabled as u8);\n\n"
    '#[cfg(any(test, feature = "durability-test-hooks"))]\n'
    "pub fn set_append_failpoint_for_test(failpoint: AppendFailpoint) {\n"
    "    APPEND_FAILPOINT.store(failpoint as u8, Ordering::SeqCst);\n"
    "}\n\n"
    '#[cfg(any(test, feature = "durability-test-hooks"))]\n'
    "fn take_append_failpoint() -> AppendFailpoint {\n"
    "    match APPEND_FAILPOINT.swap(AppendFailpoint::Disabled as u8, Ordering::SeqCst) {\n"
    "        1 => AppendFailpoint::BeforeWrite,\n"
    "        2 => AppendFailpoint::AfterWriteBeforeSync,\n"
    "        3 => AppendFailpoint::AfterSyncBeforeAck,\n"
    "        _ => AppendFailpoint::Disabled,\n"
    "    }\n"
    "}\n",
    "append failpoint insertion",
)
wal = replace_method(
    wal,
    "    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {",
    "\n    pub fn stream_from",
    """    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
        #[cfg(any(test, feature = "durability-test-hooks"))]
        let failpoint = take_append_failpoint();

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::BeforeWrite {
            return Err(anyhow!("injected append failure before WAL write"));
        }

        let offset = self.writer.append(bytes)?;

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::AfterWriteBeforeSync {
            return Err(anyhow!("injected append failure after WAL write before sync"));
        }

        self.writer.sync_all()?;

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::AfterSyncBeforeAck {
            return Err(anyhow!("injected append failure after sync before acknowledgement"));
        }

        Ok(offset)
    }
""",
    "Store::append replacement",
)
wal_path.write_text(wal)

matrix_path = root / ".github/workflows/transition-ledger-safety-matrix.yml"
matrix = matrix_path.read_text()
matrix = insert_before_once(
    matrix,
    "      - name: Run ledger unit and restart tests\n",
    "      - name: Exercise append fault boundaries\n"
    "        if: ${{ steps.fixture.outputs.status == '0' }}\n"
    "        run: cargo test -p liminal-store --features durability-test-hooks --test trustworthy_transition_fault_injection\n\n",
    "matrix fault-test insertion",
)
matrix_path.write_text(matrix)

test_path = root / "liminal-db/crates/liminal-store/tests/trustworthy_transition_fault_injection.rs"
test_path.write_text(
    r'''#![cfg(feature = "durability-test-hooks")]

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
'''
)

print("Applied marker-based append fault-injection contract")
