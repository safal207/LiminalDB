from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


def patch_wal() -> None:
    path = Path("liminal-db/crates/liminal-store/src/wal.rs")
    text = path.read_text()
    text = replace_once(
        text,
        "use anyhow::{anyhow, Context, Result};",
        "use anyhow::{Context, Result};",
        "remove obsolete anyhow macro import",
    )
    text = replace_once(
        text,
        """    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
        self.writer.append(bytes)
    }
""",
        """    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
        let offset = self.writer.append(bytes)?;
        self.writer.sync_all()?;
        Ok(offset)
    }
""",
        "make Store::append acknowledge only synced WAL bytes",
    )
    text = replace_once(
        text,
        """        self.size += record_size;
        Ok(offset)
    }

    fn rotate(&mut self) -> Result<()> {
        let next_segment = self.segment + 1;
""",
        """        self.size += record_size;
        Ok(offset)
    }

    fn sync_all(&self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }

    fn rotate(&mut self) -> Result<()> {
        self.file.sync_all()?;
        let next_segment = self.segment + 1;
""",
        "sync old WAL segment before rotation",
    )
    text = replace_once(
        text,
        """#[cfg(windows)]
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    Err(anyhow!(
        "directory synchronization is unavailable on Windows for {path:?}"
    ))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    Err(anyhow!(
        "directory synchronization is unsupported on this platform for {path:?}"
    ))
}
""",
        """#[cfg(windows)]
pub(crate) fn sync_directory(_path: &Path) -> Result<()> {
    // Rust std does not expose a portable directory fsync primitive on Windows.
    // File contents and lock files are synced; directory-entry durability remains
    // an explicit platform limitation covered by the Windows validation job.
    Ok(())
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn sync_directory(_path: &Path) -> Result<()> {
    // Preserve store availability on platforms without a directory fsync primitive.
    Ok(())
}
""",
        "preserve Store::open on Windows with an explicit durability boundary",
    )
    path.write_text(text)


def patch_transition_ledger() -> None:
    path = Path("liminal-db/crates/liminal-store/src/trustworthy_transition.rs")
    text = path.read_text()
    text = replace_once(
        text,
        """    #[error("invalid links for record kind {0:?}")]
    InvalidLinks(TransitionRecordKind),
}
""",
        """    #[error("invalid links for record kind {0:?}")]
    InvalidLinks(TransitionRecordKind),
    #[error("ledger is poisoned after an ambiguous storage failure; reopen and replay before appending")]
    PoisonedAfterStorageFailure,
}
""",
        "add fail-closed poisoned ledger error",
    )
    text = replace_once(
        text,
        """pub struct TrustworthyTransitionLedger {
    store: Store,
    state: LedgerState,
    snapshot_path: PathBuf,
}
""",
        """pub struct TrustworthyTransitionLedger {
    store: Store,
    state: LedgerState,
    snapshot_path: PathBuf,
    poisoned: bool,
}
""",
        "track ambiguous storage failure state",
    )
    text = replace_once(
        text,
        """        Ok(Self {
            store,
            state: full_replay,
            snapshot_path,
        })
""",
        """        Ok(Self {
            store,
            state: full_replay,
            snapshot_path,
            poisoned: false,
        })
""",
        "initialize healthy ledger state",
    )
    text = replace_once(
        text,
        """    ) -> Result<TransitionEvent, TransitionLedgerError> {
        let normalized = normalize_input(input)?;
""",
        """    ) -> Result<TransitionEvent, TransitionLedgerError> {
        if self.poisoned {
            return Err(TransitionLedgerError::PoisonedAfterStorageFailure);
        }
        let normalized = normalize_input(input)?;
""",
        "block append after ambiguous storage failure",
    )
    text = replace_once(
        text,
        """        let bytes = serde_cbor::to_vec(&event).map_err(encoding_error)?;
        self.store.append(&bytes).map_err(storage_error)?;
        sync_current_wal(&self.store)?;
        self.state = candidate;
""",
        """        let bytes = serde_cbor::to_vec(&event).map_err(encoding_error)?;
        if let Err(error) = self.store.append(&bytes) {
            self.poisoned = true;
            return Err(storage_error(error));
        }
        self.state = candidate;
""",
        "poison ledger on any ambiguous WAL append or sync failure",
    )
    text = replace_once(
        text,
        """            projection.observation_refs.sort();
            projection.observation_refs.dedup();
        }
""",
        """            projection.observation_refs.sort();
            projection.observation_refs.dedup();
            projection.response_integrity_ref = None;
            projection.causal_audit_ref = None;
            projection.continuity_snapshot_ref = None;
        }
""",
        "invalidate derived evidence when observation generation changes",
    )
    text = replace_once(
        text,
        """fn sync_current_wal(store: &Store) -> Result<(), TransitionLedgerError> {
    let path = store
        .wal_dir()
        .join(format!("{:08}.wal", store.current_segment()));
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(storage_error)
}

""",
        "",
        "remove duplicate path-based WAL sync",
    )
    path.write_text(text)


if __name__ == "__main__":
    patch_wal()
    patch_transition_ledger()
