# MULTI-STORE-TRUTH-001

**Status:** bounded stacked experiment  
**Base:** XTDB-LIMINALDB-001 / PR #123  
**External evidence:** `mstevens843/crashpoint@bb9cd47c4b0b02527aab7b369d17b32829cc4e20`

## Question

Can the same pre-dispatch `action_id` retain its meaning while evidence crosses three
different responsibility boundaries:

1. the pinned Crashpoint admission/readback evidence,
2. LiminalDB causal/continuity semantics,
3. XTDB knowledge-over-system-time history,

without any layer silently upgrading uncertainty into proof or replay permission?

This experiment is intentionally narrower than distributed consistency or exactly-once.

## Why this exists

`CRASHPOINT-LIMINALDB-001` established that the six published source cases can be mapped into
LiminalDB and survive snapshot + WAL replay without turning unavailable readback into
`NO_EFFECT` or `RETRY_SIDE_EFFECT`.

`XTDB-LIMINALDB-001` established a two-version bitemporal projection:

```text
CLAIM_ONLY -> published readback state
```

This experiment inserts the missing epistemic transition:

```text
CLAIM_ONLY
    ->
READBACK_UNAVAILABLE
    ->
AUTHORITATIVE_READBACK
```

where the third state is admitted only for source trials that really have `FULL` retained
readback.

## Important precision: this is not original event chronology

Crashpoint's external ledger retains payload digests and attempt IDs but does not retain a
trusted wall-clock timestamp for the external effect.

Therefore this experiment does **not** claim:

> XTDB proves exactly when the external effect occurred.

The XTDB `SYSTEM_TIME` values here are the chronology of this experiment's ingestion and
knowledge revisions.

The stable `_valid_from` value remains the source `admitted_at_utc`, which is the time anchor
that the published fixture actually supplies for the logical action.

## Three phases

### Phase A — claim only

All 18 actions are inserted with:

```text
knowledge_state = CLAIM_ONLY
external_outcome = INDETERMINATE
observation_availability = NOT_READ_BACK
externally_verified = false
continuity = REVALIDATE
```

The pre-minted source `action_id` and admission-time valid anchor are preserved.

### Phase B — unavailable readback

All 18 actions advance to:

```text
knowledge_state = READBACK_UNAVAILABLE
external_outcome = INDETERMINATE
observation_availability = UNAVAILABLE
externally_verified = false
continuity = REVALIDATE
```

For the 15 source trials whose published evidence is actually `FULL`, this is explicitly labeled:

```text
readback_stage = INJECTED_VISIBILITY_GAP
```

It is a projection-level fault injection, not a rewrite of the published Crashpoint result.

For the three published `unavailable_readback` trials it is labeled:

```text
readback_stage = SOURCE_UNAVAILABLE
```

### Phase C — authoritative readback restored

Only the 15 source-`FULL` trials receive a third system-time version.

That version must match the pinned LiminalDB mapping exactly, including:

- verified one-effect cases,
- verified zero-effect pre-dispatch stop,
- duplicate effects,
- payload mismatch,
- continuity posture.

The three source-`UNAVAILABLE` trials receive no fabricated phase C. They remain:

```text
INDETERMINATE / REVALIDATE
```

## Expected history shape

The source publication contains 18 trials:

- 15 with `FULL` readback,
- 3 with `UNAVAILABLE` readback.

Therefore the XTDB history must contain exactly:

```text
15 * 3 versions = 45
 3 * 2 versions =  6
----------------------
total             51
```

Current view: 18 rows.

Phase-A `SYSTEM_TIME AS OF`: 18 claim-only rows.

Phase-B `SYSTEM_TIME AS OF`: 18 unavailable rows.

## Cross-layer gate

The workflow deliberately re-runs the existing LiminalDB Rust mapping/replay test before the XTDB
projection is evaluated.

So one green run requires all of the following to agree:

```text
Pinned Crashpoint evidence
        |
        v
source fixture validator
        |
        v
LiminalDB mapping + replay
        |
        v
XTDB three-phase knowledge history
        |
        v
offline semantic verifier
```

## Required invariants

1. The exact same pre-dispatch `action_id` survives every projected knowledge revision.
2. Source availability provenance is retained separately from the injected Phase-B visibility gap.
3. Missing readback is always `INDETERMINATE / REVALIDATE`.
4. Missing readback never becomes `NO_EFFECT`, committed-effect absence, or replay permission.
5. Restored source readback creates a later system-time version instead of erasing the unavailable state.
6. Source-`UNAVAILABLE` trials never receive an authoritative version.
7. `_valid_from` remains stable across all versions for a logical action.
8. Resolved current rows match the pinned LiminalDB mapping.
9. History version boundaries are contiguous.

## What a green gate proves

Within the exact pins and counterfactual ingestion sequence, a green gate proves that this stack
can preserve the distinction between:

- action admission,
- client claim,
- unavailable observation,
- later authoritative observation,
- causal/continuity interpretation,
- historical knowledge state.

Most importantly, temporary evidence loss does not grant permission to repeat an external effect.

## What it does not prove

A green gate does **not** prove:

- original external-effect wall-clock time;
- distributed atomicity between SQLite, LiminalDB, and XTDB;
- globally exactly-once effects;
- host or power-loss durability across all stores;
- CrewAI, SafeAgent, LangGraph, x402, or payment-provider correctness;
- independent authorship of the Crashpoint publication;
- that XTDB should become the causal source of truth.

LiminalDB remains the causal/continuity mapping layer in this experiment. XTDB remains a
bitemporal knowledge projection.

## Canonical invariant

> **Persistence may preserve uncertainty and later evidence may refine it, but no storage layer may convert missing observation into proof that an external effect did not occur or permission to repeat it.**
