# Lotus memory import contract v0.2

This integration gives LiminalDB an offline, mutation-free acceptance check for Lotus memory artifacts emitted by `safal207/LiminalQAengineer`.

## Purpose

The LiminalQA side exports one deterministic LiminalDB-compatible `AuditEvent` per Lotus finding. This repository validates those events before any future runtime ingestion is designed.

```text
LiminalQA Lotus Decision Packet
        ↓
artifact-only AuditEvent JSONL
        ↓
LiminalDB offline validator
        ↓
current checked-out contract blob
        ↓
dry-run compatibility report
```

The validator does **not** connect to a running LiminalDB node, append to the journal, create a snapshot, accept CML memory, or execute an external action.

## FCRP-SELF-007 — Snapshot identity is not contract identity

The original v0.1 fixture was created against:

- historical LiminalDB commit: `75ef9f7f403a34c60aa2ceba4cb3c97870d73e77`
- contract path: `sdk/ts/src/protocol-types.ts`
- contract Git blob: `fd733971aaae089df770062bcf7f2c2d6d19ca1d`
- event type: `AuditEvent`

When this PR was revisited, current `main` had advanced to `0cd6e77d52787bb36a97b75ba1a37cb027268eb3`, but the exact contract path still had the **same Git blob**:

`fd733971aaae089df770062bcf7f2c2d6d19ca1d`

That distinction matters:

```text
historical adapter commit
    = provenance / repository snapshot identity

contract blob SHA
    = exact content identity of the consumed interface
```

The v0.1 validator required the adapter commit to equal one historical LiminalDB commit forever. That made ordinary repository evolution look like contract incompatibility even when the consumed bytes had not changed.

v0.2 separates the two dimensions.

## Compatibility rule

Every event must still declare:

```text
adapter.repository
adapter.commit                 # full historical provenance SHA
adapter.contract_path
adapter.contract_blob_sha
adapter.event_contract
adapter.write_mode
```

But compatibility with the validator's **current checkout** is now established as:

```text
adapter.repository == safal207/LiminalDB
AND adapter.contract_path == sdk/ts/src/protocol-types.ts
AND adapter.event_contract == AuditEvent
AND adapter.write_mode == artifact_only
AND adapter.contract_blob_sha
    == git_blob_sha(current checked-out protocol-types.ts)
```

The historical `adapter.commit` remains required and must be a full SHA, but it is not used as a semantic compatibility key.

This does **not** claim that any arbitrary historical commit contained the declared blob. It means the event is cryptographically bound to a declared contract blob and that the current checked-out consumer exposes the same exact bytes. Historical repository-commit provenance remains a separate evidence dimension.

## Exact-head report binding

CI checks out the exact PR head and passes it as `--consumer-commit`. The generated dry-run report records:

- exact consumer commit;
- current checked-out contract Git blob;
- historical adapter commit(s) declared by the events;
- `historical_snapshot_is_semantic_key: false`;
- `contract_blob_matches_current_checkout: true`.

This avoids both failure modes:

```text
wrong:
commit changed -> contract must be incompatible

also wrong:
contract changed -> old event remains compatible because repository name/path match

correct:
snapshot provenance and contract-content identity are checked separately
```

## Accepted event identity

```text
kind   = audit
actor  = liminalqa-lotus
action = lotus.finding.observed
```

## Validation

```bash
python3 -m unittest tests/test_validate_lotus_memory.py -v

python3 scripts/validate_lotus_memory.py \
  --events fixtures/lotus/valid-events.jsonl \
  --consumer-commit "$(git rev-parse HEAD)" \
  --output reports/lotus/import-check.json
```

The validator checks:

1. exact schema, actor and action;
2. historical producer/adapter commit SHA formats;
3. repository, contract path, event type and artifact-only write mode;
4. declared contract blob against the actual checked-out contract bytes;
5. timezone-bearing observation timestamps;
6. bounded and replayable evidence;
7. `audit_only` authority with every execution grant false;
8. `durable_memory == false`;
9. event SHA-256 integrity;
10. duplicate event IDs.

## Boundary

A successful dry run means only:

> The supplied JSONL is structurally and cryptographically consistent with the Lotus artifact contract, and the declared contract blob is byte-identical to the contract surface in this checked-out LiminalDB revision.

It does not mean:

- the finding is true outside its evidence scope;
- the historical adapter commit itself was independently re-fetched from GitHub;
- a CML proposal is accepted as durable memory;
- LiminalDB persisted the event;
- ownership, approval, execution, delivery, deployment or merge authority was granted.

## Next stage

PR #95 was stacked on the historical v0.1 contract and must be reconciled separately after this contract replacement is validated. A future runtime ingestion adapter remains a separately reviewed stage and must define namespace isolation, idempotency, journal transaction semantics, rejection handling, retention and rollback before it can write to a live store.
