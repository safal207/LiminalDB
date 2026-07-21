from pathlib import Path

path = Path("liminal-db/crates/liminal-store/src/wal.rs")
text = path.read_text()
old = '''        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .write(true)
            .open(&file_path)
'''
new = '''        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&file_path)
'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one WAL writer open block, found {count}")
path.write_text(text.replace(old, new, 1))
print("Applied read-write WAL handle mode for cross-platform tail recovery")
