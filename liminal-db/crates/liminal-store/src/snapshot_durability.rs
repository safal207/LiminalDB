use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::durability::{trigger_durability_failpoint, DurabilityFailpoint};
use crate::trustworthy_transition::{
    TransitionLedgerError, TransitionLedgerSnapshotInfo, TrustworthyTransitionLedger,
};
use crate::wal::sync_directory;

pub trait CrashSafeTransitionSnapshotExt {
    fn write_snapshot_crash_safe(
        &mut self,
        created_at_ms: u64,
    ) -> Result<TransitionLedgerSnapshotInfo, TransitionLedgerError>;
}

impl CrashSafeTransitionSnapshotExt for TrustworthyTransitionLedger {
    fn write_snapshot_crash_safe(
        &mut self,
        created_at_ms: u64,
    ) -> Result<TransitionLedgerSnapshotInfo, TransitionLedgerError> {
        let snapshot_path = self.snapshot_path().to_path_buf();
        recover_interrupted_snapshot_replace(&snapshot_path).map_err(storage_error)?;
        let backup = rollback_path(&snapshot_path)?;
        let parent = parent(&snapshot_path)?;

        if snapshot_path.exists() {
            fs::rename(&snapshot_path, &backup).map_err(storage_error)?;
            sync_directory(parent).map_err(storage_error)?;
        }

        match self.write_snapshot(created_at_ms) {
            Ok(info) => {
                sync_directory(parent).map_err(storage_error)?;
                if backup.exists() {
                    fs::remove_file(&backup).map_err(storage_error)?;
                    sync_directory(parent).map_err(storage_error)?;
                }
                Ok(info)
            }
            Err(error) => {
                if !snapshot_path.exists() && backup.exists() {
                    let _ = fs::rename(&backup, &snapshot_path);
                    let _ = sync_directory(parent);
                }
                Err(error)
            }
        }
    }
}

pub fn replace_snapshot_bytes_crash_safe(
    snapshot_path: &Path,
    bytes: &[u8],
) -> Result<(), TransitionLedgerError> {
    recover_interrupted_snapshot_replace(snapshot_path).map_err(storage_error)?;
    let parent = parent(snapshot_path)?;
    let temporary = temporary_path(snapshot_path)?;
    let backup = rollback_path(snapshot_path)?;

    if temporary.exists() {
        fs::remove_file(&temporary).map_err(storage_error)?;
    }

    let midpoint = bytes.len() / 2;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(storage_error)?;
    file.write_all(&bytes[..midpoint]).map_err(storage_error)?;
    file.flush().map_err(storage_error)?;
    trigger_durability_failpoint(DurabilityFailpoint::SnapshotDuringTempWrite);
    file.write_all(&bytes[midpoint..]).map_err(storage_error)?;
    file.flush().map_err(storage_error)?;
    file.sync_all().map_err(storage_error)?;
    drop(file);

    trigger_durability_failpoint(DurabilityFailpoint::SnapshotAfterFileSyncBeforeRename);

    if snapshot_path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(storage_error)?;
        }
        fs::rename(snapshot_path, &backup).map_err(storage_error)?;
    }

    if let Err(error) = fs::rename(&temporary, snapshot_path) {
        if !snapshot_path.exists() && backup.exists() {
            let _ = fs::rename(&backup, snapshot_path);
        }
        return Err(storage_error(error));
    }

    trigger_durability_failpoint(DurabilityFailpoint::SnapshotAfterRenameBeforeDirectorySync);
    sync_directory(parent).map_err(storage_error)?;

    if backup.exists() {
        fs::remove_file(&backup).map_err(storage_error)?;
        sync_directory(parent).map_err(storage_error)?;
    }
    Ok(())
}

pub fn recover_interrupted_snapshot_replace(snapshot_path: &Path) -> Result<(), std::io::Error> {
    let parent = snapshot_path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "snapshot has no parent")
    })?;
    let temporary = temporary_path_io(snapshot_path)?;
    let backup = rollback_path_io(snapshot_path)?;

    if snapshot_path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
            sync_directory(parent).map_err(to_io_error)?;
        }
    } else if backup.exists() {
        fs::rename(&backup, snapshot_path)?;
        sync_directory(parent).map_err(to_io_error)?;
    }

    if temporary.exists() {
        fs::remove_file(&temporary)?;
        sync_directory(parent).map_err(to_io_error)?;
    }
    Ok(())
}

fn parent(path: &Path) -> Result<&Path, TransitionLedgerError> {
    path.parent()
        .ok_or_else(|| TransitionLedgerError::Storage("snapshot path has no parent".to_owned()))
}

fn temporary_path(path: &Path) -> Result<PathBuf, TransitionLedgerError> {
    temporary_path_io(path).map_err(storage_error)
}

fn rollback_path(path: &Path) -> Result<PathBuf, TransitionLedgerError> {
    rollback_path_io(path).map_err(storage_error)
}

fn temporary_path_io(path: &Path) -> Result<PathBuf, std::io::Error> {
    sibling(path, ".tmp")
}

fn rollback_path_io(path: &Path) -> Result<PathBuf, std::io::Error> {
    sibling(path, ".rollback")
}

fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, std::io::Error> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "snapshot has no parent")
    })?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid snapshot filename",
            )
        })?;
    Ok(parent.join(format!(".{name}{suffix}")))
}

fn storage_error(error: impl std::fmt::Display) -> TransitionLedgerError {
    TransitionLedgerError::Storage(error.to_string())
}

fn to_io_error(error: anyhow::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn interrupted_replace_restores_backup_when_destination_is_missing() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("state.snap");
        fs::write(&path, b"old").expect("old snapshot");
        let backup = rollback_path_io(&path).expect("backup path");
        fs::rename(&path, &backup).expect("move to backup");

        recover_interrupted_snapshot_replace(&path).expect("recover");
        assert_eq!(fs::read(&path).expect("read"), b"old");
        assert!(!backup.exists());
    }

    #[test]
    fn installed_destination_wins_over_leftover_backup() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("state.snap");
        let backup = rollback_path_io(&path).expect("backup path");
        fs::write(&path, b"new").expect("new snapshot");
        fs::write(&backup, b"old").expect("backup snapshot");

        recover_interrupted_snapshot_replace(&path).expect("recover");
        assert_eq!(fs::read(&path).expect("read"), b"new");
        assert!(!backup.exists());
    }

    #[test]
    fn replacement_updates_existing_file_cross_platform() {
        let directory = tempdir().expect("tempdir");
        let path = directory.path().join("state.snap");
        fs::write(&path, b"old").expect("old snapshot");

        replace_snapshot_bytes_crash_safe(&path, b"new snapshot").expect("replace");
        assert_eq!(fs::read(&path).expect("read"), b"new snapshot");
        assert!(!temporary_path_io(&path).expect("temp").exists());
        assert!(!rollback_path_io(&path).expect("backup").exists());
    }
}
