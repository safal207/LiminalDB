# Transition Ledger Crash-Consistency and Platform Matrix v0.1

**Status:** Draft stacked durability profile  
**Tracking issue:** [LiminalDB #91](https://github.com/safal207/LiminalDB/issues/91)  
**Base ledger:** [LiminalDB PR #89](https://github.com/safal207/LiminalDB/pull/89)  
**Signed checkpoints:** [LiminalDB PR #92](https://github.com/safal207/LiminalDB/pull/92)

## Purpose

This profile defines the process-crash durability boundary for LiminalDB's
trustworthy-transition ledger and publishes reproducible evidence on:

```text
Ubuntu
Windows
macOS
```

It covers:

- exclusive writer ownership;
- WAL write, flush, and file synchronization boundaries;
- recovery after a crash before the in-memory projection is updated;
- WAL segment rotation interruption;
- partial WAL framing;
- cross-platform snapshot replacement and rollback recovery;
- restart replay after abrupt child-process termination.

## Merge stack

This implementation is deliberately stacked:

```text
PR #89  durable ledger, WAL, snapshots, replay
    ↓
PR #92  signed checkpoints and external anchors
    ↓
PR #93  crash consistency and platform evidence
```

The layers remain separate because local crash consistency, external
anti-rollback, and transition semantics prove different properties.

## Acknowledged append boundary

`Store::append` now completes in this order:

```text
record framing and write
        ↓
File::flush
        ↓
File::sync_all
        ↓
return success to caller
```

The event is acknowledged only after `sync_all` succeeds.

The matrix proves that an append that returned success is present after closing
and reopening the ledger. A crash before the return may leave either no new
record or a complete unacknowledged record, depending on which boundary was
crossed. Recovery must never produce a fabricated partial event.

## Single-writer root ownership

`Store::open` obtains an OS-level exclusive lock on:

```text
.liminaldb-writer.lock
```

The lock file contains diagnostic process metadata, but the operating-system
lock—not the text—is authoritative.

The lock is held for the lifetime of `Store`. Process death releases it through
the operating system. A second process or second `Store` instance for the same
root fails closed instead of opening another WAL writer.

This lock assumes the underlying filesystem implements local advisory file
locking correctly. Distributed and hostile network filesystems require an
external lease or consensus mechanism.

## Feature-gated failpoints

Failpoints are active only when the crate is built with:

```text
durability-failpoints
```

Production builds without this feature ignore the failpoint environment.

The stable failpoint names are:

```text
before_wal_write
after_wal_write_before_flush
after_wal_flush_before_sync
after_wal_sync_before_return
snapshot_during_temp_write
snapshot_after_file_sync_before_rename
snapshot_after_rename_before_directory_sync
wal_segment_rotation
```

When selected, the child process writes an optional marker and exits with code
`86` through `std::process::exit`. Rust destructors and unwinding do not run.

## WAL crash expectations

| Failpoint | Expected restart state |
|---|---|
| `before_wal_write` | Previous committed prefix only. |
| `after_wal_write_before_flush` | Complete new record is visible after process crash; it was not acknowledged. |
| `after_wal_flush_before_sync` | Complete new record is visible after process crash; power-loss durability is not claimed. |
| `after_wal_sync_before_return` | Synced new record is recovered although caller never received success. |
| Normal return | Acknowledged event must be recovered. |
| `wal_segment_rotation` | Previous segment remains replayable; an empty next segment may exist. |

The matrix also appends a truncated record with a complete length prefix but
missing payload/CRC. Replay must reject it through WAL framing.

## Snapshot replacement API

Repeated cross-platform snapshot replacement should use:

```rust
CrashSafeTransitionSnapshotExt::write_snapshot_crash_safe
```

The v0.1 raw snapshot writer used direct rename semantics that differ between
Unix and Windows when the destination already exists. The crash-safe extension
uses explicit rollback ownership:

```text
current snapshot → .rollback
new bytes        → .tmp
.tmp             → current snapshot
sync directory where supported
remove .rollback
```

Recovery rules are deterministic:

- destination exists plus rollback exists: installed destination wins;
- destination missing plus rollback exists: restore rollback;
- stale temporary file: remove it;
- no snapshot after an interrupted replacement: full WAL replay remains the
  source of truth.

The matrix stages real old and new trustworthy-transition snapshot bytes and
crashes at:

1. partial temporary-file write;
2. full file sync before rename;
3. installed destination before directory synchronization.

After every crash, reopening must reconstruct the same two-event ledger state.

## WAL rotation

Rotation creates and synchronizes the next segment before changing the active
writer state. A crash at the rotation failpoint may leave an empty higher-number
segment. Restart selects that segment for future writes while replay still reads
all complete earlier records.

## Cross-platform synchronization scope

### Linux

- WAL and snapshot files use `File::sync_all`.
- Directory metadata is synchronized by opening the directory and calling
  `sync_all`.
- Evidence applies to the GitHub-hosted Linux filesystem used by CI.

### macOS

- WAL and snapshot files use `File::sync_all`.
- Directory metadata is synchronized through the Unix directory handle.
- Evidence applies to the GitHub-hosted macOS filesystem used by CI.

### Windows

- WAL and snapshot files use `File::sync_all`.
- Exclusive writer locking is provided through the Windows file-locking backend
  used by `fs2`.
- Rust's standard library does not expose a portable directory-fsync primitive
  on Windows; the matrix therefore claims process-crash recovery and file-data
  synchronization, not metadata persistence across sudden power loss.
- Snapshot replacement uses rollback files rather than assuming Unix overwrite
  semantics for `rename`.

## Evidence runner

Run locally:

```bash
python tools/run_transition_durability_matrix.py \
  --output artifacts/transition-ledger-durability.json
```

The runner:

1. builds the child harness with `durability-failpoints`;
2. creates a fresh root for every scenario;
3. launches a separate process;
4. confirms failpoint marker and exit code;
5. reopens the root in a new process;
6. records the recovered event/record count;
7. writes a JSON receipt.

Receipt schema:

```text
org.liminaldb.transition-durability-evidence.v0.1
```

Each CI platform uploads a separate artifact:

```text
transition-ledger-durability-ubuntu-latest
transition-ledger-durability-windows-latest
transition-ledger-durability-macos-latest
```

## Evidence cases

The generated receipt includes:

1. acknowledged append survives restart;
2. four WAL append failpoints;
3. three snapshot replacement failpoints;
4. interrupted segment rotation;
5. partial WAL record rejection;
6. second-process writer rejection;
7. writer lock release after process death.

## Claim boundary

This profile proves behavior under abrupt process termination on the listed
GitHub-hosted runner environments. It does not simulate or prove:

- sudden power loss;
- disk-controller cache loss;
- filesystem implementation bugs;
- network filesystem lease correctness;
- malicious kernel or administrator behavior;
- distributed consensus;
- external rollback resistance;
- correctness of authorization, observation, response-integrity, causal, or
  continuity records.

External rollback resistance is supplied by signed checkpoints and caller-trusted
anchors from PR #92.

## Canonical invariant

> After the documented acknowledgment boundary, restart must recover the event.
> Before that boundary, restart may recover either the old prefix or a complete
> additional record, but never an invented or silently corrupted transition.
