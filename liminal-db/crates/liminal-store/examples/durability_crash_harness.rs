use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use liminal_store::{
    replace_snapshot_bytes_crash_safe, sha256_ref, CrashSafeTransitionSnapshotExt, Store,
    TransitionEventInput, TransitionLinks, TransitionRecordKind, TrustworthyTransitionLedger,
};

const TRANSITION_ID: &str = "transition-durability-matrix";
const SUBJECT_ID: &str = "agent:durability-matrix";

fn auth_ref() -> String {
    sha256_ref(b"durability-auth-record")
}

fn authorization() -> TransitionEventInput {
    TransitionEventInput {
        transition_id: TRANSITION_ID.to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        kind: TransitionRecordKind::Authorization,
        record_ref: auth_ref(),
        payload_digest: sha256_ref(b"durability-auth-payload"),
        links: TransitionLinks::default(),
        dimensions: None,
        side_effect_committed: Some(false),
        captured_at_ms: 100,
    }
}

fn observation() -> TransitionEventInput {
    TransitionEventInput {
        transition_id: TRANSITION_ID.to_owned(),
        subject_id: SUBJECT_ID.to_owned(),
        kind: TransitionRecordKind::Observation,
        record_ref: sha256_ref(b"durability-observation-record"),
        payload_digest: sha256_ref(b"durability-observation-payload"),
        links: TransitionLinks {
            authorization_ref: Some(auth_ref()),
            ..TransitionLinks::default()
        },
        dimensions: None,
        side_effect_committed: Some(false),
        captured_at_ms: 200,
    }
}

fn prepare_ledger(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    ledger.append(authorization())?;
    ledger.write_snapshot_crash_safe(110)?;
    Ok(())
}

fn prepare_snapshot_case(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    ledger.append(authorization())?;
    ledger.write_snapshot_crash_safe(110)?;
    ledger.append(observation())?;
    Ok(())
}

fn append_observation(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    ledger.append(observation())?;
    Ok(())
}

fn crash_append(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    append_observation(root)?;
    eprintln!("append unexpectedly returned without triggering failpoint");
    std::process::exit(87);
}

fn crash_snapshot(root: &Path, failpoint: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut ledger = TrustworthyTransitionLedger::open(root)?;
    let path = ledger.snapshot_path().to_path_buf();
    let old_bytes = fs::read(&path)?;

    ledger.write_snapshot_crash_safe(220)?;
    let new_bytes = fs::read(&path)?;
    replace_snapshot_bytes_crash_safe(&path, &old_bytes)?;

    env::set_var("LIMINALDB_FAILPOINT", failpoint);
    replace_snapshot_bytes_crash_safe(&path, &new_bytes)?;
    eprintln!("snapshot replacement unexpectedly returned");
    std::process::exit(87);
}

fn inspect_ledger(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let ledger = TrustworthyTransitionLedger::open(root)?;
    println!("EVENT_COUNT={}", ledger.event_count());
    println!("HEAD={}", ledger.head_event_hash().unwrap_or("NONE"));
    Ok(())
}

fn prepare_rotation(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::open_with_rotation(root, 1)?;
    store.append(b"baseline")?;
    Ok(())
}

fn crash_rotation(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = Store::open_with_rotation(root, 1)?;
    store.append(b"second")?;
    eprintln!("rotation unexpectedly returned without triggering failpoint");
    std::process::exit(87);
}

fn inspect_store(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(root)?;
    let records: Vec<Vec<u8>> = store
        .stream_from(liminal_store::Offset::start())?
        .collect::<Result<Vec<_>, _>>()?;
    println!("RECORD_COUNT={}", records.len());
    Ok(())
}

fn hold_lock(root: &Path, ready: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let _store = Store::open(root)?;
    fs::write(ready, b"ready\n")?;
    loop {
        thread::sleep(Duration::from_millis(100));
    }
}

fn try_open(root: &Path) -> ! {
    match Store::open(root) {
        Ok(_) => {
            eprintln!("writer lock unexpectedly acquired");
            std::process::exit(72);
        }
        Err(error) => {
            eprintln!("writer lock rejected: {error}");
            std::process::exit(73);
        }
    }
}

fn path_arg(args: &[String], index: usize) -> Result<PathBuf, String> {
    args.get(index)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing argument {index}"))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let command = args.get(1).ok_or("missing command")?;
    let root = path_arg(&args, 2)?;

    match command.as_str() {
        "prepare-ledger" => prepare_ledger(&root),
        "prepare-snapshot" => prepare_snapshot_case(&root),
        "append" => append_observation(&root),
        "crash-append" => crash_append(&root),
        "crash-snapshot" => {
            let failpoint = args.get(3).ok_or("missing snapshot failpoint")?;
            crash_snapshot(&root, failpoint)
        }
        "inspect-ledger" => inspect_ledger(&root),
        "prepare-rotation" => prepare_rotation(&root),
        "crash-rotation" => crash_rotation(&root),
        "inspect-store" => inspect_store(&root),
        "hold-lock" => {
            let ready = path_arg(&args, 3)?;
            hold_lock(&root, &ready)
        }
        "try-open" => try_open(&root),
        other => Err(format!("unknown command: {other}").into()),
    }
}
