from pathlib import Path


path = Path("liminal-db/crates/liminal-store/src/trustworthy_transition.rs")
text = path.read_text()
old = '''        raw_store
            .append(&serde_cbor::to_vec(&event).expect("encode event"))
            .expect("append bad semantic event");
        sync_current_wal(&raw_store).expect("sync");
        drop(raw_store);
'''
new = '''        raw_store
            .append(&serde_cbor::to_vec(&event).expect("encode event"))
            .expect("append and sync bad semantic event");
        drop(raw_store);
'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"semantic event test compatibility: expected one match, found {count}")
path.write_text(text.replace(old, new, 1))
