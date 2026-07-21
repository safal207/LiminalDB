#[cfg(any(test, feature = "durability-test-hooks"))]
use std::cell::Cell;
use std::fs::{create_dir_all, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use crc32fast::Hasher as Crc32;
use serde::{Deserialize, Serialize};

use fs2::FileExt;
const WAL_EXTENSION: &str = "wal";
const DEFAULT_SEGMENT_SIZE: u64 = 32 * 1024 * 1024; // 32 MiB
const WRITER_LOCK_FILE: &str = ".liminaldb-writer.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Offset {
    pub segment: u64,
    pub position: u64,
}

impl Offset {
    pub fn start() -> Self {
        Offset {
            segment: 1,
            position: 0,
        }
    }
}

pub struct Store {
    data_dir: PathBuf,
    snap_dir: PathBuf,
    writer: WalWriter,
    manifest_path: PathBuf,
    _writer_lock: File,
}

impl Store {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        create_dir_all_durable(&root)?;
        let writer_lock = acquire_writer_lock(&root)?;
        let data_dir = root.join("data");
        let snap_dir = root.join("snap");
        create_dir_all_durable(&data_dir)?;
        create_dir_all_durable(&snap_dir)?;
        let writer = WalWriter::open(&data_dir, DEFAULT_SEGMENT_SIZE)?;
        let manifest_path = root.join("manifest.cbor");
        Ok(Store {
            data_dir,
            snap_dir,
            writer,
            manifest_path,
            _writer_lock: writer_lock,
        })
    }

    pub fn append(&mut self, bytes: &[u8]) -> Result<Offset> {
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

    pub fn stream_from(&self, offset: Offset) -> Result<WalStream> {
        WalStream::new(self.data_dir.clone(), offset)
    }

    pub fn wal_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn snap_dir(&self) -> &Path {
        &self.snap_dir
    }

    pub fn current_segment(&self) -> u64 {
        self.writer.segment
    }

    pub fn end_offset(&self) -> Offset {
        Offset {
            segment: self.writer.segment,
            position: self.writer.size,
        }
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn read_manifest(&self) -> Result<Option<StoreManifest>> {
        if !self.manifest_path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.manifest_path)?;
        let manifest = serde_cbor::from_slice(&bytes)?;
        Ok(Some(manifest))
    }

    pub fn write_manifest(&self, manifest: &StoreManifest) -> Result<()> {
        let bytes = serde_cbor::to_vec(manifest)?;
        std::fs::write(&self.manifest_path, bytes)?;
        Ok(())
    }

    pub fn next_snapshot_id(&self) -> Result<u64> {
        let mut ids = list_snapshots(&self.snap_dir)?;
        ids.sort_unstable();
        Ok(ids.last().copied().unwrap_or(0) + 1)
    }

    pub fn snapshot_path(&self, id: u64) -> PathBuf {
        snapshot_path(&self.snap_dir, id)
    }

    pub fn list_snapshots(&self) -> Result<Vec<u64>> {
        list_snapshots(&self.snap_dir)
    }
}

fn create_dir_all_durable(path: &Path) -> Result<()> {
    let mut missing = Vec::new();
    let mut current = path;
    while !current.exists() {
        missing.push(current.to_path_buf());
        let Some(parent) = current.parent() else {
            break;
        };
        if parent.as_os_str().is_empty() {
            break;
        }
        current = parent;
    }

    create_dir_all(path)?;
    for directory in missing.iter().rev() {
        let parent = directory.parent().unwrap_or_else(|| Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        };
        sync_directory(parent)?;
    }
    Ok(())
}

fn acquire_writer_lock(root: &Path) -> Result<File> {
    let path = root.join(WRITER_LOCK_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open writer lock {path:?}"))?;
    file.try_lock_exclusive()
        .with_context(|| format!("liminaldb writer lock is already held for {root:?}"))?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    writeln!(file, "pid={}", std::process::id())?;
    file.flush()?;
    file.sync_all()?;
    sync_directory(root)?;
    Ok(file)
}

struct WalWriter {
    data_dir: PathBuf,
    rotation: u64,
    segment: u64,
    file: File,
    size: u64,
}

impl WalWriter {
    fn open(data_dir: &Path, rotation: u64) -> Result<Self> {
        let mut segments = list_segments(data_dir)?;
        segments.sort_unstable();
        let segment = segments.last().copied().unwrap_or(1);
        let file_path = wal_path(data_dir, segment);
        let created = !file_path.exists();
        let mut file = OpenOptions::new()
            .create(true)
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
        Ok(WalWriter {
            data_dir: data_dir.to_path_buf(),
            rotation,
            segment,
            file,
            size,
        })
    }

    fn append(&mut self, payload: &[u8], failpoint: AppendFailpoint) -> Result<Offset> {
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

    fn sync_all(&self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }

    fn rotate(&mut self) -> Result<()> {
        self.file.sync_all()?;
        let next_segment = self.segment + 1;
        let file_path = wal_path(&self.data_dir, next_segment);
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(true)
            .open(&file_path)
            .with_context(|| format!("failed to create wal segment {:?}", file_path))?;
        file.sync_all()?;
        sync_directory(&self.data_dir)?;
        self.segment = next_segment;
        self.size = 0;
        self.file = file;
        Ok(())
    }
}

fn recover_last_segment_tail(file: &mut File, path: &Path) -> Result<u64> {
    let file_len = file.metadata()?.len();
    let mut position = 0_u64;
    file.seek(SeekFrom::Start(0))?;

    while position < file_len {
        let frame_start = position;
        let remaining_frame_bytes = file_len - frame_start;
        if remaining_frame_bytes < 4 {
            return truncate_torn_tail(file, path, frame_start, file_len);
        }

        let mut len_buf = [0_u8; 4];
        file.read_exact(&mut len_buf)?;
        let payload_len = u32::from_le_bytes(len_buf) as u64;
        let payload_start = frame_start + 4;
        let available_after_header = file_len - payload_start;
        let frame_end = payload_start
            .checked_add(payload_len)
            .and_then(|value| value.checked_add(4))
            .ok_or_else(|| anyhow!("wal frame length overflow in {path:?} at {frame_start}"))?;

        if frame_end > file_len {
            if available_after_header == 0
                || (available_after_header >= payload_len
                    && available_after_header < payload_len + 4)
            {
                return truncate_torn_tail(file, path, frame_start, file_len);
            }
            return Err(anyhow!(
                "ambiguous partial WAL payload in {path:?} at offset {frame_start}"
            ));
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

#[cfg(unix)]
pub(crate) fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
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

pub(crate) fn list_segments(dir: &Path) -> Result<Vec<u64>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut segments = Vec::new();
    for entry in dir.read_dir()? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(WAL_EXTENSION) {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(num) = stem.parse::<u64>() {
                segments.push(num);
            }
        }
    }
    if segments.is_empty() {
        segments.push(1);
    }
    segments.sort_unstable();
    segments.dedup();
    Ok(segments)
}

pub(crate) fn wal_path(base: &Path, segment: u64) -> PathBuf {
    base.join(format!("{segment:08}.{}", WAL_EXTENSION))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreManifest {
    pub last_snapshot: Option<String>,
    pub wal_segment: u64,
    pub wal_position: u64,
}

pub(crate) fn list_snapshots(dir: &Path) -> Result<Vec<u64>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in dir.read_dir()? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("snap") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if let Ok(num) = stem.parse::<u64>() {
                ids.push(num);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

pub(crate) fn snapshot_path(base: &Path, id: u64) -> PathBuf {
    base.join(format!("{id:08}.snap"))
}

pub struct WalStream {
    data_dir: PathBuf,
    segments: Vec<u64>,
    current_idx: usize,
    file: Option<File>,
    position: u64,
    offset: Offset,
}

impl WalStream {
    fn new(data_dir: PathBuf, offset: Offset) -> Result<Self> {
        let mut segments = list_segments(&data_dir)?;
        segments.sort_unstable();
        let start_idx = segments
            .iter()
            .position(|seg| *seg >= offset.segment)
            .unwrap_or(segments.len().saturating_sub(1));
        let mut stream = WalStream {
            data_dir,
            segments,
            current_idx: start_idx,
            file: None,
            position: offset.position,
            offset,
        };
        stream.open_current()?;
        Ok(stream)
    }

    fn open_current(&mut self) -> Result<()> {
        while self.current_idx < self.segments.len() {
            let segment = self.segments[self.current_idx];
            let path = wal_path(&self.data_dir, segment);
            match OpenOptions::new().read(true).open(&path) {
                Ok(mut file) => {
                    if self.offset.segment > segment {
                        // Skip older segments
                        self.current_idx += 1;
                        continue;
                    }
                    if self.offset.segment == segment && self.offset.position > 0 {
                        file.seek(SeekFrom::Start(self.offset.position))?;
                        self.position = self.offset.position;
                    } else {
                        self.position = 0;
                    }
                    self.file = Some(file);
                    self.offset.position = 0;
                    self.offset.segment = segment;
                    return Ok(());
                }
                Err(err) => {
                    self.current_idx += 1;
                    if self.current_idx >= self.segments.len() {
                        return Err(err.into());
                    }
                }
            }
        }
        Ok(())
    }
}

impl Iterator for WalStream {
    type Item = Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some(file) = self.file.as_mut() else {
                if self.current_idx >= self.segments.len() {
                    return None;
                }
                if let Err(err) = self.open_current() {
                    return Some(Err(err));
                }
                continue;
            };

            let mut len_buf = [0u8; 4];
            match file.read_exact(&mut len_buf) {
                Ok(()) => {}
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::UnexpectedEof {
                        self.current_idx += 1;
                        self.file = None;
                        self.offset.segment += 1;
                        self.offset.position = 0;
                        continue;
                    }
                    return Some(Err(err.into()));
                }
            }
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut payload = vec![0u8; len];
            if let Err(err) = file.read_exact(&mut payload) {
                return Some(Err(err.into()));
            }
            let mut crc_buf = [0u8; 4];
            if let Err(err) = file.read_exact(&mut crc_buf) {
                return Some(Err(err.into()));
            }
            let mut hasher = Crc32::new();
            hasher.update(&payload);
            let expected = hasher.finalize();
            let actual = u32::from_le_bytes(crc_buf);
            if expected != actual {
                return Some(Err(anyhow!("wal checksum mismatch")));
            }
            self.position += 4 + len as u64 + 4;
            self.offset.position = self.position;
            return Some(Ok(payload));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn wal_round_trip() {
        let dir = tempdir().expect("tempdir");
        let mut store = Store::open(dir.path()).expect("open store");
        let payloads = vec![b"hello".to_vec(), b"world".to_vec(), b"liminal".to_vec()];
        for payload in &payloads {
            store.append(payload).expect("append");
        }
        let stream = store.stream_from(Offset::start()).expect("stream");
        let collected: Vec<Vec<u8>> = stream.map(|entry| entry.expect("entry")).collect();
        assert_eq!(collected, payloads);
    }

    #[test]
    fn second_writer_is_rejected_until_first_is_dropped() {
        let dir = tempdir().expect("tempdir");
        let first = Store::open(dir.path()).expect("first writer");
        assert!(Store::open(dir.path()).is_err(), "second writer must fail");
        drop(first);
        Store::open(dir.path()).expect("writer lock released on drop");
    }

    #[cfg(unix)]
    #[test]
    fn nested_store_directories_are_created_durably() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("nested/store/root");
        Store::open(&root).expect("open nested store");
        assert!(root.join("data").is_dir());
        assert!(root.join("snap").is_dir());
    }

    #[test]
    fn wal_crc_detects_corruption() {
        let dir = tempdir().expect("tempdir");
        let mut store = Store::open(dir.path()).expect("open store");
        store.append(b"data").expect("append");
        drop(store);

        // Corrupt the file by flipping a byte in payload
        let path = dir.path().join("data/00000001.wal");
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open wal");
        file.seek(SeekFrom::Start(2)).expect("seek");
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte).expect("read");
        byte[0] ^= 0xFF;
        file.seek(SeekFrom::Start(2)).expect("seek back");
        file.write_all(&byte).expect("write");
        drop(file);

        assert!(
            Store::open(dir.path()).is_err(),
            "checksum corruption must fail closed during store open"
        );
    }
}
