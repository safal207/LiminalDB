# Crashpoint Action Readback → LiminalDB Mapping v0.1

**System case:** `CRASHPOINT-LIMINALDB-001`  
**Status:** bounded external-evidence mapping experiment  
**LiminalDB base:** `61b02fc81e0cb5cf1f1ed4658ecff58f683cb728`  
**External publication:** `mstevens843/crashpoint@bb9cd47c4b0b02527aab7b369d17b32829cc4e20`

## Question

Can the existing LiminalDB trustworthy-transition model preserve the safety-relevant distinctions in the published Crashpoint pre-dispatch-action/readback fixture without changing core ledger semantics?

The external fixture contains six cases, three predeclared trials each:

```text
clean
effect_before_lost_receipt
stopped_before_effect
naive_retry
payload_mismatch
unavailable_readback
```

The pinned Crashpoint manifest reports 18/18 trials matching its frozen prediction. This experiment does not rerun that producer or its offline verifier. It independently checks the pinned manifest/prediction facts and then tests whether the existing LiminalDB ledger can retain the mapped continuity state through snapshot + full replay.

## Source pin

The mapping fixture pins:

```text
repository:
  mstevens843/crashpoint

publication commit:
  bb9cd47c4b0b02527aab7b369d17b32829cc4e20

bundle:
  evidence/action_readback/action_readback_self_reviewed_v2

prediction sha256:
  1881868b0cb9b5bcfdd4b034c8bff9c0d5e50c46e6f8cce77f43e9f5aa85496d
```

The dedicated validator checks the downloaded pinned `manifest.json` and `prediction.json` against the local fixture before the Rust mapping test runs.

## Mapping

| Crashpoint case | Source boundary | LiminalDB continuity mapping |
|---|---|---|
| `clean` | one verified matching effect | `OBSERVED_EXECUTED / VERIFIED / VALID / REPORT_ONLY` |
| `effect_before_lost_receipt` | one verified effect, client receipt lost | `OBSERVED_EXECUTED / UNKNOWN / VALID / REPORT_ONLY` |
| `stopped_before_effect` | independently verified empty receiver after pre-dispatch stop | `NOT_OBSERVED / NOT_EVALUATED / VALID / RETRY_SIDE_EFFECT` |
| `naive_retry` | two matching effects for one logical action | two observation refs + `PARTIAL / VALID / BLOCKED` |
| `payload_mismatch` | effect exists under action ID but payload digest differs | `FAILED / INVALID / BLOCKED` |
| `unavailable_readback` | readback unavailable; effect count indeterminate | `NOT_OBSERVED / UNKNOWN / NOT_EVALUATED / REVALIDATE` |

The mapping is deliberately conservative. It does not turn a process-local success claim into external truth.

## Critical invariant: unavailable is not absent

The highest-risk boundary is:

```text
observation_availability = UNAVAILABLE
external_outcome = INDETERMINATE
externally_verified = false
```

LiminalDB maps that case to:

```text
continuity_posture = REVALIDATE
```

It must never become:

```text
NO_EFFECT
RETRY_SIDE_EFFECT
```

merely because external readback is unavailable.

The current transition projection exposes `side_effect_committed` as a monotonic boolean. In this experiment, `false` means **no positive committed-effect assertion has been established in the projection**. It is not interpreted as independent proof that an effect did not occur. The pinned source classification remains the evidence authority for `UNAVAILABLE / INDETERMINATE`.

## Observation multiplicity

The source `naive_retry` case records two real effects. The mapping therefore uses two LiminalDB observation records under the same authorization root.

This checks that retry multiplicity is not collapsed into a single success status:

```text
one logical action
  ├── observation/effect A
  └── observation/effect B
          ↓
continuity = BLOCKED
```

## Payload mismatch

The source `payload_mismatch` case has:

```text
correct action ID
different payload
```

The mapping intentionally fails response integrity and causal validity instead of treating action-ID equality as success:

```text
response_integrity = FAILED
causal_validity = INVALID
continuity_posture = BLOCKED
```

## Replay test

For every mapped case the Rust regression:

1. opens a fresh `TrustworthyTransitionLedger`;
2. appends an authorization root;
3. appends one or more observations;
4. appends response-integrity and causal-audit records;
5. appends a continuity snapshot;
6. writes a LiminalDB snapshot;
7. closes the writer;
8. reopens the ledger;
9. requires full replay to reproduce the exact final projection.

No core ledger enum or validation rule is changed by this experiment.

## Validation

Pinned source verification:

```bash
python3 scripts/validate_crashpoint_action_readback_fixture.py \
  --fixture liminal-db/crates/liminal-store/tests/fixtures/crashpoint_action_readback_v0.1.json \
  --manifest /tmp/crashpoint-manifest.json \
  --prediction /tmp/crashpoint-prediction.json
```

LiminalDB mapping/replay:

```bash
cargo test --locked -p liminal-store \
  --test crashpoint_action_readback_fixture -- --nocapture
```

The dedicated GitHub Actions workflow performs both steps against the exact pinned external commit and uploads the validation report.

## What a green gate proves

Within the declared pins:

1. the local mapping fixture still matches the six published Crashpoint classifications and all 18 published trial records;
2. the frozen prediction bytes still match the pinned SHA-256;
3. LiminalDB can preserve one-vs-multiple observation structure;
4. payload mismatch is not folded into successful execution;
5. a lost client receipt does not erase independently observed execution;
6. unavailable external readback remains indeterminate and maps to revalidation, not blind retry;
7. the mapped continuity state survives LiminalDB snapshot + full WAL replay without modifying core semantics.

## What it does not prove

A green gate does **not** prove:

- that LiminalDB independently reran the Crashpoint producer or offline verifier;
- that the Crashpoint author is an independent trust domain from their own fixture;
- CrewAI behavior;
- SafeAgent behavior;
- a native CrewAI/SafeAgent adapter;
- distributed fencing;
- sudden host/power-loss durability;
- payment-provider correctness;
- global exactly-once external effects;
- production permission to retry a real-world operation from a zero-effect fixture alone.

The `stopped_before_effect → RETRY_SIDE_EFFECT` mapping is limited to this fixture's explicitly verified pre-dispatch/empty-receiver condition.

## Next boundary

If this gate stays green, the next useful experiment is not another synthetic fixture. It is a bounded comparison that feeds the same pre-minted logical action + authoritative readback evidence into an external control mechanism and preserves the raw mapping result.

## Canonical invariant

> **Unknown external outcome must remain unknown until stronger evidence arrives. Persistence and replay may preserve that uncertainty; they may not convert it into permission to repeat a side effect.**
