from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


root = Path(".")
wal_path = root / "liminal-db/crates/liminal-store/src/wal.rs"
wal = wal_path.read_text()

wal = replace_once(
    wal,
    '''use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "durability-test-hooks"))]
use std::sync::atomic::{AtomicU8, Ordering};
''',
    '''use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "durability-test-hooks"))]
use std::cell::Cell;
''',
    "thread-local import",
)

wal = replace_once(
    wal,
    '''#[cfg(any(test, feature = "durability-test-hooks"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AppendFailpoint {
    Disabled = 0,
    BeforeWrite = 1,
    AfterWriteBeforeSync = 2,
    AfterSyncBeforeAck = 3,
}

#[cfg(any(test, feature = "durability-test-hooks"))]
static APPEND_FAILPOINT: AtomicU8 = AtomicU8::new(AppendFailpoint::Disabled as u8);

#[cfg(any(test, feature = "durability-test-hooks"))]
pub fn set_append_failpoint_for_test(failpoint: AppendFailpoint) {
    APPEND_FAILPOINT.store(failpoint as u8, Ordering::SeqCst);
}

#[cfg(any(test, feature = "durability-test-hooks"))]
fn take_append_failpoint() -> AppendFailpoint {
    match APPEND_FAILPOINT.swap(AppendFailpoint::Disabled as u8, Ordering::SeqCst) {
        1 => AppendFailpoint::BeforeWrite,
        2 => AppendFailpoint::AfterWriteBeforeSync,
        3 => AppendFailpoint::AfterSyncBeforeAck,
        _ => AppendFailpoint::Disabled,
    }
}
''',
    '''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendFailpoint {
    Disabled,
    BeforeWrite,
    AfterLengthWrite,
    AfterPayloadWrite,
    AfterWriteBeforeSync,
    AfterSyncBeforeAck,
}

#[cfg(any(test, feature = "durability-test-hooks"))]
thread_local! {
    static APPEND_FAILPOINT: Cell<AppendFailpoint> = const {
        Cell::new(AppendFailpoint::Disabled)
    };
}

#[cfg(any(test, feature = "durability-test-hooks"))]
pub fn set_append_failpoint_for_test(failpoint: AppendFailpoint) {
    APPEND_FAILPOINT.with(|slot| slot.set(failpoint));
}

#[cfg(any(test, feature = "durability-test-hooks"))]
fn take_append_failpoint() -> AppendFailpoint {
    APPEND_FAILPOINT.with(|slot| {
        let failpoint = slot.get();
        slot.set(AppendFailpoint::Disabled);
        failpoint
    })
}
''',
    "failpoint state",
)

wal = replace_once(
    wal,
    '''    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
        #[cfg(any(test, feature = "durability-test-hooks"))]
        let failpoint = take_append_failpoint();

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::BeforeWrite {
            return Err(anyhow!("injected append failure before WAL write"));
        }

        let offset = self.writer.append(bytes)?;

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::AfterWriteBeforeSync {
            return Err(anyhow!(
                "injected append failure after WAL write before sync"
            ));
        }

        self.writer.sync_all()?;

        #[cfg(any(test, feature = "durability-test-hooks"))]
        if failpoint == AppendFailpoint::AfterSyncBeforeAck {
            return Err(anyhow!(
                "injected append failure after sync before acknowledgement"
            ));
        }

        Ok(offset)
    }
''',
    '''    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
        #[cfg(any(test, feature = "durability-test-hooks"))]
        let failpoint = take_append_failpoint();
        #[cfg(not(any(test, feature = "durability-test-hooks")))]
        let failpoint = AppendFailpoint::Disabled;

        if failpoint == AppendFailpoint::BeforeWrite {
            return Err(anyhow!("injected append failure before WAL write"));
        }

        let offset = self.writer.append(bytes, failpoint)?;

        if failpoint == AppendFailpoint::AfterWriteBeforeSync {
            return Err(anyhow!(
                "injected append failure after WAL write before sync"
            ));
        }

        self.writer.sync_all()?;

        if failpoint == AppendFailpoint::AfterSyncBeforeAck {
            return Err(anyhow!(
                "injected append failure after sync before acknowledgement"
            ));
        }

        Ok(offset)
    }
''',
    "Store::append",
)

wal = replace_once(
    wal,
    '''        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&file_path)
            .with_context(|| format!("failed to open wal segment {:?}", file_path))?;
        if created {
            file.sync_all()?;
            sync_directory(data_dir)?;
        }
        let size = file.metadata()?.len();
''',
    '''        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .write(true)
            .open(&file_path)
            .with_context(|| format!("failed to open wal segment {:?}", file_path))?;
        let size = if created {
            file.sync_all()?;
            sync_directory(data_dir)?;
            0
        } else {
            recover_last_segment_tail(&mut file, &file_path)?
        };
''',
    "WalWriter::open recovery",
)

wal = replace_once(
    wal,
    '''    fn append(&mut self, payload: &[u8]) -> Result<Offset> {
        let record_size = 4u64 + payload.len() as u64 + 4;
        if self.size + record_size > self.rotation {
            self.rotate()?;
        }
        let offset = Offset {
            segment: self.segment,
            position: self.size,
        };
        let len = payload.len() as u32;
        let mut hasher = Crc32::new();
        hasher.update(payload);
        let checksum = hasher.finalize();
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(payload)?;
        self.file.write_all(&checksum.to_le_bytes())?;
        self.file.flush()?;
        self.size += record_size;
        Ok(offset)
    }
''',
    '''    fn append(&mut self, payload: &[u8], failpoint: AppendFailpoint) -> Result<Offset> {
        let record_size = 4u64 + payload.len() as u64 + 4;
        if self.size + record_size > self.rotation {
            self.rotate()?;
        }
        let offset = Offset {
            segment: self.segment,
            position: self.size,
        };
        let len = payload.len() as u32;
        let mut hasher = Crc32::new();
        hasher.update(payload);
        let checksum = hasher.finalize();
        self.file.write_all(&len.to_le_bytes())?;
        if failpoint == AppendFailpoint::AfterLengthWrite {
            return Err(anyhow!("injected append failure after WAL length write"));
        }
        self.file.write_all(payload)?;
        if failpoint == AppendFailpoint::AfterPayloadWrite {
            return Err(anyhow!("injected append failure after WAL payload write"));
        }
        self.file.write_all(&checksum.to_le_bytes())?;
        self.file.flush()?;
        self.size += record_size;
        Ok(offset)
    }
''',
    "WalWriter::append partial writes",
)

recovery_helpers = '''
fn recover_last_segment_tail(file: &mut File, path: &Path) -> Result<u64> {
    let file_len = file.metadata()?.len();
    let mut position = 0_u64;
    file.seek(SeekFrom::Start(0))?;

    while position < file_len {
        let frame_start = position;
        if file_len - frame_start < 4 {
            return truncate_torn_tail(file, path, frame_start, file_len);
        }

        let mut len_buf = [0_u8; 4];
        file.read_exact(&mut len_buf)?;
        let payload_len = u32::from_le_bytes(len_buf) as u64;
        let frame_end = frame_start
            .checked_add(4)
            .and_then(|value| value.checked_add(payload_len))
            .and_then(|value| value.checked_add(4))
            .ok_or_else(|| anyhow!("wal frame length overflow in {path:?} at {frame_start}"))?;

        if frame_end > file_len {
            return truncate_torn_tail(file, path, frame_start, file_len);
        }

        let mut hasher = Crc32::new();
        let mut remaining = payload_len;
        let mut buffer = [0_u8; 8192];
        while remaining > 0 {
            let chunk_len = remaining.min(buffer.len() as u64) as usize;
            file.read_exact(&mut buffer[..chunk_len])?;
            hasher.update(&buffer[..chunk_len]);
            remaining -= chunk_len as u64;
        }

        let mut crc_buf = [0_u8; 4];
        file.read_exact(&mut crc_buf)?;
        let expected = hasher.finalize();
        let actual = u32::from_le_bytes(crc_buf);
        if expected != actual {
            return Err(anyhow!(
                "wal checksum mismatch in {path:?} at offset {frame_start}"
            ));
        }
        position = frame_end;
    }

    file.seek(SeekFrom::End(0))?;
    Ok(position)
}

fn truncate_torn_tail(file: &mut File, path: &Path, valid_len: u64, file_len: u64) -> Result<u64> {
    file.set_len(valid_len).with_context(|| {
        format!("failed to truncate torn WAL tail in {path:?} from {file_len} to {valid_len}")
    })?;
    file.sync_all()
        .with_context(|| format!("failed to sync repaired WAL segment {path:?}"))?;
    file.seek(SeekFrom::End(0))?;
    Ok(valid_len)
}

'''
wal = replace_once(
    wal,
    "#[cfg(unix)]\npub(crate) fn sync_directory",
    recovery_helpers + "#[cfg(unix)]\npub(crate) fn sync_directory",
    "torn-tail recovery helpers",
)

wal = replace_once(
    wal,
    '''        let store = Store::open(dir.path()).expect("reopen store");
        let mut stream = store.stream_from(Offset::start()).expect("stream");
        assert!(
            stream.next().unwrap().is_err(),
            "corruption must be detected"
        );
''',
    '''        assert!(
            Store::open(dir.path()).is_err(),
            "checksum corruption must fail closed during store open"
        );
''',
    "checksum corruption test",
)

wal_path.write_text(wal)

test_path = root / "liminal-db/crates/liminal-store/tests/trustworthy_transition_fault_injection.rs"
test = test_path.read_text()
test = replace_once(
    test,
    '''    let cases = [
        (AppendFailpoint::BeforeWrite, 0_u64),
        (AppendFailpoint::AfterWriteBeforeSync, 1_u64),
        (AppendFailpoint::AfterSyncBeforeAck, 1_u64),
    ];
''',
    '''    let cases = [
        (AppendFailpoint::BeforeWrite, 0_u64),
        (AppendFailpoint::AfterLengthWrite, 0_u64),
        (AppendFailpoint::AfterPayloadWrite, 0_u64),
        // The complete frame survives in this non-crash error simulation.
        // A real pre-sync crash may lose it; reopen always follows the bytes that survived.
        (AppendFailpoint::AfterWriteBeforeSync, 1_u64),
        (AppendFailpoint::AfterSyncBeforeAck, 1_u64),
    ];
''',
    "partial-frame cases",
)

test += '''

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
'''
test_path.write_text(test)

print("Applied PR99 torn-tail recovery and thread-local failpoints")
