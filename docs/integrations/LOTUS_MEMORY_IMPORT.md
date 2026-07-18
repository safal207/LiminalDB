# Lotus memory import contract v0.1

This integration gives LiminalDB an offline, mutation-free acceptance check for
Lotus memory artifacts emitted by `safal207/LiminalQAengineer`.

## Purpose

The LiminalQA side exports one deterministic LiminalDB-compatible `AuditEvent`
per Lotus finding. This repository validates those events before any future
runtime ingestion is designed.

```text
LiminalQA Lotus Decision Packet
        ↓
artifact-only AuditEvent JSONL
        ↓
LiminalDB offline validator
        ↓
dry-run acceptance report
```

The validator does **not** connect to a running LiminalDB node, append to the
journal, create a snapshot, accept CML memory, or execute an external action.

## Exact producer contract

Producer PR: `safal207/LiminalQAengineer#66`

Pinned LiminalDB surface:

- repository: `safal207/LiminalDB`
- commit: `75ef9f7f403a34c60aa2ceba4cb3c97870d73e77`
- contract path: `sdk/ts/src/protocol-types.ts`
- contract blob SHA: `fd733971aaae089df770062bcf7f2c2d6d19ca1d`
- event type: `AuditEvent`

The accepted event identity is:

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
  --output reports/lotus/import-check.json
```

The validator checks:

1. exact schema, actor, action, adapter and contract pins;
2. full commit and SHA-256 formats;
3. timezone-bearing observation timestamps;
4. bounded and replayable evidence;
5. `audit_only` authority with every execution grant false;
6. `durable_memory == false`;
7. `write_mode == artifact_only`;
8. event SHA-256 integrity;
9. duplicate event IDs.

## Boundary

A successful dry run means only:

> The supplied JSONL is structurally and cryptographically consistent with the
> pinned Lotus-to-LiminalDB artifact contract.

It does not mean:

- the finding is true outside its evidence scope;
- a CML proposal is accepted as durable memory;
- LiminalDB has persisted the event;
- ownership, approval, execution, delivery, deployment or merge authority was
  granted.

## Next stage

A later separately reviewed change may introduce a runtime ingestion adapter.
That stage must define namespace isolation, idempotency, journal transaction
semantics, rejection handling, retention and rollback before it can write to a
live store.
