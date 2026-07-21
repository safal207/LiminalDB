from pathlib import Path

path = Path("liminal-db/crates/liminal-store/src/wal.rs")
text = path.read_text()
start_marker = "fn recover_last_segment_tail(file: &mut File, path: &Path) -> Result<u64> {\n"
end_marker = "fn truncate_torn_tail("
start = text.find(start_marker)
if start < 0:
    raise SystemExit("recovery helper start not found")
end = text.find(end_marker, start)
if end < 0:
    raise SystemExit("recovery helper end not found")

replacement = '''fn recover_last_segment_tail(file: &mut File, path: &Path) -> Result<u64> {
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

'''
path.write_text(text[:start] + replacement + text[end:])
print("Refined torn-tail versus corruption boundary")
