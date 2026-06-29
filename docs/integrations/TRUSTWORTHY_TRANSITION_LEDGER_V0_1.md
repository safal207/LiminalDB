# LiminalDB Trustworthy-Transition Ledger v0.1

**Status:** Draft interoperability profile  
**Tracking issue:** [#88](https://github.com/safal207/LiminalDB/issues/88)  
**Implementation PR:** [#89](https://github.com/safal207/LiminalDB/pull/89)

## Purpose

This profile defines durable event sourcing for one trustworthy agent transition.
It stores references produced by independent ecosystem components without
collapsing their responsibilities:

```text
authorization_record
        ↓
observation_record(s)
        ↓
response_integrity_record
        ↓
causal_audit_record
        ↓
continuity_snapshot_record
```

LiminalDB records and replays this chain. It does not issue authority, execute a
tool, judge a response claim, calculate causal validity, or decide whether work
may resume.

## Storage layers

Two independent integrity layers are used:

1. **Physical WAL framing** — length prefix plus CRC-32 detects damaged WAL
   records and segment corruption.
2. **Semantic event chain** — every transition event contains the previous
   event SHA-256 and its own SHA-256 over deterministic CBOR bytes.

A valid WAL checksum therefore cannot hide a modified semantic event body.
Likewise, a valid semantic event cannot repair a damaged WAL frame.

The ledger must use a dedicated storage root. It must not share a WAL directory
with `DiskJournal`, because the two journals encode different record types.

## Record kinds

### `authorization`

Starts an authorization epoch. The first authorization has no parent. A later
authorization for the same transition must explicitly reference the current
authorization in `links.authorization_ref`.

A new epoch clears only the current derived downstream pointers. Historical WAL
events and global record ownership remain intact.

### `observation`

Must reference the current authorization for the same transition and subject.
Its own `record_ref` is added to the monotonically growing current observation
set.

### `response_integrity`

Must reference:

- the current authorization;
- the exact sorted set of all current observation references.

It cannot omit an observation or import an observation from another transition.

### `causal_audit`

Must reference:

- the current authorization;
- the exact current observation set;
- the current response-integrity record, when one exists.

### `continuity_snapshot`

Must reference the exact current evidence boundary:

- authorization;
- all observations;
- response integrity;
- causal audit;
- previous continuity snapshot, when one exists.

A continuity snapshot must carry all independent dimensions.

## Independent dimensions

The ledger preserves, but does not reinterpret:

```text
authority
execution
response_integrity
causal_validity
continuity_posture
```

Examples of valid independent states include:

```text
VALID + OBSERVED_EXECUTED + FAILED + VALID + REMEDIATE_RESPONSE
EXPIRED_AT_REPORT + OBSERVED_EXECUTED + VERIFIED + VALID + REPORT_ONLY
VALID + NOT_OBSERVED + NOT_EVALUATED + VALID + CONTINUE_SIDE_EFFECT
```

## Durable event envelope

Each WAL payload contains:

```text
schema
profile
sequence
transition_id
subject_id
record kind
record_ref
payload_digest
parent links
optional independent dimensions
optional side_effect_committed
captured_at_ms
previous_event_hash
event_hash
```

All external references use lowercase:

```text
sha256:<64 hexadecimal characters>
```

Record references are globally unique inside one ledger root.

## Deterministic projection

Replay derives one `TransitionProjection` per transition:

```text
transition_id
subject_id
current authorization_ref
authorization_epoch
sorted observation_refs
current response_integrity_ref
current causal_audit_ref
current continuity_snapshot_ref
independent dimensions
side_effect_committed
last sequence
last event hash
event count
```

The projection is rebuildable and is not an independent source of truth.

## Monotonic invariants

The ledger rejects:

- duplicate global `record_ref` values;
- missing authorization roots;
- wrong or missing parent links;
- cross-transition or cross-subject ancestry;
- incomplete observation evidence sets;
- implicit authorization replacement;
- `side_effect_committed: true -> false`;
- `OBSERVED_EXECUTED -> any other execution state`;
- continuity snapshots without all dimensions;
- event sequence gaps;
- previous-event-hash mismatches;
- semantic event-hash mismatches.

## Snapshot format

The ledger writes one atomically replaced snapshot under its dedicated snapshot
directory. The snapshot binds:

```text
snapshot schema and profile
exact WAL offset
next event sequence
semantic chain head
all deterministic projections
global record-owner index
projection digest
snapshot digest
created_at_ms
```

The file is written to a temporary path, synchronized, renamed atomically, and
the containing directory is synchronized.

## Recovery algorithm

Opening the ledger performs both paths:

```text
verified snapshot + WAL tail replay
                versus
full WAL replay from Offset::start()
```

The resulting complete states must be equal. If they differ, recovery fails with
`ReplayProjectionMismatch`.

This keeps the snapshot an accelerator rather than an authority source.

## Stable failure classes

| Failure | Meaning |
|---|---|
| `SequenceMismatch` | Event ordering is discontinuous. |
| `PreviousEventHashMismatch` | Semantic event ancestry is broken. |
| `EventHashMismatch` | Event body does not match its SHA-256. |
| `SnapshotDigestMismatch` | Snapshot body was modified. |
| `SnapshotProjectionDigestMismatch` | Snapshot projection material was modified or inconsistent. |
| `ReplayProjectionMismatch` | Snapshot-assisted recovery differs from full WAL replay. |
| `DuplicateRecordReference` | A record ref was reused anywhere in the ledger. |
| `MissingAuthorization` | A dependent record has no authorization root. |
| `MissingParent` | A referenced parent is not present. |
| `ParentMismatch` | A supplied link is not the exact current parent. |
| `CrossTransitionReference` | A parent belongs to another transition or subject. |
| `ReauthorizationWithoutSupersession` | A new authorization did not name the old authorization. |
| `ObservationSetMismatch` | A downstream record omitted or substituted observations. |
| `SideEffectRollback` | Committed execution was rewritten as uncommitted. |
| `ExecutionRollback` | Executed state was rewritten as unobserved or another status. |

## Fixture coverage

The versioned JSON fixture covers:

1. full chain snapshot and restart;
2. cross-transition parent substitution;
3. omitted observation evidence;
4. duplicate global record references;
5. committed-side-effect rollback;
6. execution-dimension rollback;
7. implicit reauthorization;
8. tampered snapshot digest;
9. valid WAL CRC with invalid semantic event hash;
10. snapshot-tail replay equality with full replay.

## Validation

```bash
cd liminal-db
cargo check --workspace
cargo test -q -p liminal-store --no-fail-fast
cargo run -q -p liminal-store --example trustworthy_transition_ledger
```

## Security and trust boundary

The ledger provides local durability and deterministic integrity verification.
It does not yet provide:

- external transparency-log anchoring;
- signer identity or signature verification;
- protection against an attacker rolling back both the snapshot and all WAL
  segments to a mutually consistent older copy;
- distributed consensus;
- replication quorum guarantees;
- proof that upstream record contents are semantically correct.

External anti-rollback anchoring and signed checkpoint manifests are appropriate
follow-ups for v0.2.

## Canonical invariant

> Durable storage may preserve a decision and its evidence. It may never invent,
> upgrade, erase, or silently reconnect their meaning during replay.
