use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurabilityFailpoint {
    BeforeWalWrite,
    AfterWalWriteBeforeFlush,
    AfterWalFlushBeforeSync,
    AfterWalSyncBeforeReturn,
    SnapshotDuringTempWrite,
    SnapshotAfterFileSyncBeforeRename,
    SnapshotAfterRenameBeforeDirectorySync,
    WalSegmentRotation,
}

impl DurabilityFailpoint {
    pub const ALL: [Self; 8] = [
        Self::BeforeWalWrite,
        Self::AfterWalWriteBeforeFlush,
        Self::AfterWalFlushBeforeSync,
        Self::AfterWalSyncBeforeReturn,
        Self::SnapshotDuringTempWrite,
        Self::SnapshotAfterFileSyncBeforeRename,
        Self::SnapshotAfterRenameBeforeDirectorySync,
        Self::WalSegmentRotation,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeWalWrite => "before_wal_write",
            Self::AfterWalWriteBeforeFlush => "after_wal_write_before_flush",
            Self::AfterWalFlushBeforeSync => "after_wal_flush_before_sync",
            Self::AfterWalSyncBeforeReturn => "after_wal_sync_before_return",
            Self::SnapshotDuringTempWrite => "snapshot_during_temp_write",
            Self::SnapshotAfterFileSyncBeforeRename => "snapshot_after_file_sync_before_rename",
            Self::SnapshotAfterRenameBeforeDirectorySync => {
                "snapshot_after_rename_before_directory_sync"
            }
            Self::WalSegmentRotation => "wal_segment_rotation",
        }
    }
}

impl fmt::Display for DurabilityFailpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "durability-failpoints")]
pub fn trigger_durability_failpoint(point: DurabilityFailpoint) {
    crash_if(point);
}

#[cfg(not(feature = "durability-failpoints"))]
pub fn trigger_durability_failpoint(_point: DurabilityFailpoint) {}

#[cfg(feature = "durability-failpoints")]
pub(crate) fn crash_if(point: DurabilityFailpoint) {
    let selected = std::env::var("LIMINALDB_FAILPOINT").ok();
    if selected.as_deref() != Some(point.as_str()) {
        return;
    }

    if let Ok(marker_path) = std::env::var("LIMINALDB_FAILPOINT_MARKER") {
        let marker = format!("{}\n", point.as_str());
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(marker_path)
        {
            use std::io::Write as _;
            let _ = file.write_all(marker.as_bytes());
            let _ = file.sync_all();
        }
    }

    std::process::exit(86);
}

#[cfg(not(feature = "durability-failpoints"))]
pub(crate) fn crash_if(_point: DurabilityFailpoint) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failpoint_names_are_unique_and_stable() {
        let mut names = std::collections::BTreeSet::new();
        for point in DurabilityFailpoint::ALL {
            assert!(names.insert(point.as_str()));
        }
        assert_eq!(names.len(), 8);
    }
}
