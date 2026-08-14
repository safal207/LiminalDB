# ProofPath Durable Ingestion v0.1

**Status:** local/test-only persistence contract  
**System case:** `FCRP-SYSTEM-005`  
**Upstream boundary:** `FCRP-SYSTEM-004` / `be860d7a6ca089a4514d12a8108d27873b04dfb9`

## Purpose

SYSTEM-004 proved that native ProofPath evidence can cross into LiminalDB as a validated `AuditEvent` artifact while deliberately stopping before persistence.

SYSTEM-005 tests the next boundary without changing that result:

```text
native ProofPath verification
        ↓
canonical artifact-only LiminalDB validation
        ↓
separate local/test storage admission
        ↓
ProofPathDurableLedger
        ↓
WAL append + fsync
        ↓
process restart
        ↓
full replay
        ↓
exact byte recovery + idempotency check
```

The artifact validator still reports:

```text
mode = dry_run
write_performed = false
durable_memory_accepted = false
live_ingestion_performed = false
```

That report does **not** grant persistence. The durable ledger requires a separate `storage_admission_ref` and hard-codes the write scope to `local_test_only`.

## Why a dedicated ledger

The existing `TrustworthyTransitionLedger` is already a strong native durability primitive: CRC-framed WAL records, semantic SHA-256 chaining, synchronized writes, deterministic replay, snapshots, torn-tail recovery, and fault-injection coverage.

However, that ledger intentionally stores external `record_ref` / `payload_digest` values rather than arbitrary external evidence bytes.

SYSTEM-005 needs to prove a stronger statement:

> the exact accepted ProofPath artifact and its exact LiminalDB admission report can be recovered byte-for-byte after restart.

`ProofPathDurableLedger` therefore reuses the same `Store` / WAL implementation but gives ProofPath evidence its own record schema and physically isolated namespace root.

## Durable record

Each WAL record binds:

```text
schema / profile
sequence
namespace
ingestion_key
logical_operation_id
source_event_sha256
source_receipt_ref
admission_report_sha256
producer repository / capability / canonical commit
consumer repository / import-contract commit / contract blob
valid_time_ms
transaction_time_ms
exact source_event_bytes
exact admission_report_bytes
storage_admission_ref
persistence_scope = local_test_only
storage_write_authorized = true
execution_authorized = false
mutation_authorized = false
external_effects_authorized = false
previous_record_hash
record_hash
```

The consumer identities remain deliberately separate:

```text
LiminalDB import-contract commit
    = provenance of the dedicated ProofPath artifact contract

AuditEvent Git blob
    = semantic compatibility identity
```

Neither is inferred from the other.

## Idempotency model

The durable ingestion key is derived from:

```text
namespace
+ logical_operation_id
+ fixed semantic record kind
```

It is **not** derived from payload bytes.

This matters after an ambiguous acknowledgement:

```text
WAL sync succeeds
→ acknowledgement path fails
→ caller sees an error
→ reopen + replay finds the durable record
→ same semantic retry returns ALREADY_PRESENT
```

If the same operation key is reused with changed source evidence or changed admission evidence:

```text
same key + different semantic evidence
→ IdempotencyConflict
→ no second append
```

A later retry timestamp or a new local/test admission reference cannot rewrite the first durable `transaction_time_ms`.

## Bi-temporal boundary

The record carries two distinct clocks:

```text
valid_time_ms
    = when the represented ProofPath observation is valid / observed

transaction_time_ms
    = when LiminalDB first durably records that accepted artifact
```

The ledger rejects:

```text
transaction_time_ms < valid_time_ms
```

A duplicate retry does not create a new transaction time because it does not create a second durable event.

## Namespace isolation

`ProofPathDurableLedger::open(root, namespace)` stores each namespace under a distinct physical root:

```text
<root>/proofpath-durable-v0.1/<namespace>/
```

Namespaces are validated as bounded single path segments and cannot be `.` or `..`.

The same logical operation can therefore exist independently in two namespaces without sharing WAL state or writer locks.

## Recovery and acknowledgement semantics

The underlying LiminalDB `Store::append`:

1. frames payload bytes with length + CRC-32;
2. writes and flushes the complete frame;
3. calls `sync_all()` before returning success;
4. supports fault injection before write, during partial frame construction, before sync, and after sync before acknowledgement;
5. repairs clearly torn tails on reopen and fails closed on ambiguous corruption.

`ProofPathDurableLedger` advances its in-memory index only after `Store::append` returns success.

After any storage error, the writer is poisoned until reopen. This prevents an ambiguous append result from being followed by an unsafe second append in the same process.

The critical SYSTEM-005 regression is:

```text
AfterSyncBeforeAck
→ append returns Storage(error)
→ same process refuses another append
→ process closes
→ reopen replays one durable record
→ retry returns ALREADY_PRESENT
→ event_count remains 1
```

This is the persistence analogue of the Post-Commit False Failure class: an acknowledgement failure must not become a duplicate durable effect.

## Replay verification

Opening the durable ledger performs full WAL replay from `Offset::start()` and rejects records whose:

- schema/profile changed;
- sequence or previous-record hash is discontinuous;
- record SHA-256 is invalid;
- namespace or ingestion key changed;
- exact payload bytes no longer match their SHA-256;
- producer capability identity changed;
- consumer import-contract or contract-blob identity changed;
- temporal ordering is invalid;
- persistence scope is not `local_test_only`;
- execution, mutation, or external-effect authority becomes true;
- ingestion key appears twice in the WAL.

The recovered record exposes the exact original source event and admission-report bytes for byte-for-byte comparison.

## Authority boundary

SYSTEM-005 separates three different facts:

```text
artifact accepted by validator
        ≠
storage admission granted
        ≠
execution authority granted
```

The only positive authority represented by this v0.1 record is:

```text
storage_write_authorized = true
persistence_scope = local_test_only
```

The following remain false and replay-enforced:

```text
execution_authorized
mutation_authorized
external_effects_authorized
```

No service endpoint, production ingestion route, deployment path, financial action, credential authority, or external mutation is added by this contract.

## Validation

Core tests:

```bash
cargo test -p liminal-store --test proofpath_durable_ingestion
cargo test -p liminal-store --features durability-test-hooks \
  --test proofpath_durable_fault_injection
```

Local harness:

```text
proofpath_durable_ingestion ingest ...
proofpath_durable_ingestion inspect ...
```

The dedicated CI gate rebuilds the pinned SYSTEM-004 ProofPath event, runs the canonical artifact validator, persists it through a local namespace, opens it again in a separate process, compares recovered bytes, retries idempotently, and attempts a conflicting rewrite.

## What a green SYSTEM-005 proves

Within the pinned local/test revisions:

1. exact accepted ProofPath event bytes survive a durable LiminalDB WAL append and process restart;
2. the exact artifact-admission report survives with them;
3. `logical_operation_id` remains the durable idempotency coordinate;
4. `valid_time_ms` and first-write `transaction_time_ms` remain distinct and replayable;
5. ambiguous post-sync acknowledgement failure does not create a duplicate on retry;
6. changed evidence under the same operation fails closed;
7. namespace roots are physically isolated;
8. producer provenance and consumer semantic compatibility remain separate;
9. local/test persistence does not escalate execution, mutation, or external-effect authority.

## What remains unproven

A green v0.1 does **not** prove:

- a production API or service ingestion route;
- multi-process concurrent writers to one namespace beyond the existing exclusive writer lock;
- distributed replication or quorum durability;
- external transparency-log anchoring;
- protection against rollback of all mutually consistent local WAL history by an attacker;
- retention/compaction semantics for ProofPath payload bytes;
- tenant authorization policy beyond physical namespace isolation;
- truth of the upstream ProofPath incident;
- production persistence authority.

Those are separate future gates.

## Canonical invariant

> **Durability may preserve accepted evidence. It may not turn acceptance into truth, persistence into execution authority, or retry ambiguity into a second durable effect.**
