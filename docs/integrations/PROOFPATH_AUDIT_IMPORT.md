# ProofPath SCIG verification → LiminalDB AuditEvent contract v0.1

## Purpose

This contract is the first native consumer boundary for moving a **verified ProofPath SCIG result** toward LiminalDB without pretending that validation is persistence.

```text
ProofPath canonical SCIG capability
        ↓
native proofpath-scig result = VALID
        ↓
bounded bridge receipt
        ↓
LiminalDB AuditEvent projection
        ↓
current AuditEvent contract blob check
        ↓
dry-run compatibility report
```

The contract is deliberately `artifact_only`.

A PASS means the supplied AuditEvent is structurally and cryptographically consistent with this ProofPath import profile and the current checked-out LiminalDB `AuditEvent` surface. It does **not** mean LiminalDB persisted anything.

## Why Lotus v0.2 is not reused

The existing Lotus contract is intentionally narrow:

```text
actor  = liminalqa-lotus
action = lotus.finding.observed
```

Re-labeling a ProofPath SCIG verification as a Lotus finding would create a false green result by changing semantic identity at the adapter boundary.

Therefore ProofPath gets its own explicit event profile:

```text
actor  = proofpath-scig-native-verifier
action = proofpath.scig.verification.observed
schema = liminaldb-proofpath-audit-event-v0.1
```

This is an interface addition, not a new decision or execution authority.

## Identity separation

The contract preserves four independent facts:

```text
ProofPath capability commit
    = producer / verifier provenance identity

SCIG + bridge receipt digests
    = exact evidence identity

LiminalDB AuditEvent contract blob
    = current semantic compatibility identity

LiminalDB repository commit
    = consumer snapshot provenance
```

None of these identities substitutes for the others.

The historical consumer commit is required as provenance, but compatibility is determined by the exact checked-out Git blob of:

`sdk/ts/src/protocol-types.ts`

This carries forward FCRP-SELF-007:

> repository snapshot identity is not semantic contract identity.

## Accepted source

The event must declare:

- repository: `safal207/ProofPath`;
- capability: `proofpath.scig.v0.1`;
- canonical capability commit: `685d50e256a5125a21f4c4584b326411caaa64ad`;
- native verifier: `proofpath-scig`;
- native result: `VALID`;
- verification class: `native_recomputed`;
- exact SCIG SHA-256;
- exact bridge-receipt SHA-256;
- one preserved `logical_operation_id`.

The system-level producer is responsible for proving that the current ProofPath capability manifest still marks that capability `CANONICAL` and default-consumable before producing the event.

## Authority and persistence boundary

Every event must remain:

```text
authority.mode = evidence_only
execution       = false
mutation        = false
persistence     = false
deployment      = false
merge           = false

persistence.write_mode      = artifact_only
durable_memory              = false
live_ingestion               = false
namespace_mutation           = false
```

A successful validation therefore proves **import compatibility**, not durable state.

```text
proof accepted as compatible artifact
        ≠
proof accepted as truth
        ≠
proof persisted durably
        ≠
persistence authority
```

## Validation

```bash
python3 -m unittest tests/test_validate_proofpath_audit_import.py -v

python3 scripts/validate_proofpath_audit_import.py \
  --events reports/proofpath/proofpath-audit-events.jsonl \
  --consumer-commit "$(git rev-parse HEAD)" \
  --output reports/proofpath/import-check.json
```

The validator checks:

1. exact event actor/action/schema;
2. `correlationId == logical_operation_id`;
3. exact canonical ProofPath SCIG capability identity;
4. native `VALID` result and native verifier identity;
5. SHA-256 shape for SCIG and bridge receipt;
6. bounded, replayable, source-bound evidence;
7. zero execution/mutation/persistence/deployment/merge authority;
8. artifact-only persistence flags;
9. exact current LiminalDB AuditEvent contract blob;
10. event SHA-256 integrity and duplicate IDs.

## Negative controls

The contract fails closed when:

- logical operation identity changes at the boundary;
- a non-VALID native ProofPath result is presented as importable;
- any authority flag becomes true;
- durable/live persistence is claimed;
- the current AuditEvent contract bytes drift;
- the event is modified without recomputing its integrity hash;
- duplicate event IDs appear.

## What this does not prove

- ProofPath proves universal truth about the underlying incident;
- the bridge receipt was produced by an independent organization;
- LiminalDB performed a journal append or snapshot write;
- replay after restart preserves the event;
- namespace isolation, idempotency, transaction atomicity, retention or rollback for live ingestion;
- publication, deployment, merge, financial or execution authority.

## Next falsifiable stage

A future durable-ingestion contract may be designed only after this artifact boundary is stable. That stage must separately prove:

- namespace and tenant isolation;
- idempotent append identity;
- journal transaction semantics;
- valid-time / transaction-time preservation;
- restart replay;
- rejection and rollback behavior;
- no conversion of persisted evidence into execution authority.
