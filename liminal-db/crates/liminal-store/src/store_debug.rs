use std::fmt;

use crate::wal::Store;

impl fmt::Debug for Store {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Store")
            .field("wal_dir", &self.wal_dir())
            .field("snap_dir", &self.snap_dir())
            .field("current_segment", &self.current_segment())
            .field("end_offset", &self.end_offset())
            .finish_non_exhaustive()
    }
}
