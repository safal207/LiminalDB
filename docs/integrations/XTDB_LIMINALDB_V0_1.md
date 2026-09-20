# XTDB ↔ LiminalDB bitemporal projection v0.1

**System case:** `XTDB-LIMINALDB-001`  
**Status:** bounded projection experiment  
**Stacked on:** `CRASHPOINT-LIMINALDB-001` / PR #122  
**XTDB:** stable `v2.1.0` container

## Purpose

This experiment asks a narrower question than replacing the LiminalDB core:

> Can XTDB preserve the distinction between **what the client claimed**, **what was later learned from authoritative readback**, and **when that knowledge changed**, while the final safety classification remains aligned with LiminalDB?

The source evidence is the same pinned 18-trial Crashpoint action/readback bundle used by `CRASHPOINT-LIMINALDB-001`.

The experiment does not treat XTDB as the source of safety semantics. LiminalDB remains the canonical mapping for:

- execution observation;
- response integrity;
- causal validity;
- continuity posture;
- committed-side-effect interpretation.

XTDB is tested as an independent bitemporal projection.

## Why XTDB

XTDB automatically maintains two temporal dimensions for every row:

```text
SYSTEM_TIME
  = when XTDB learned / stored a version

VALID_TIME
  = when that version is considered effective in the represented domain
```

For this experiment:

```text
_valid_from
  = source action admitted_at_utc

_system_from
  = when the projection version entered XTDB
```

That allows one logical action to preserve both:

```text
T1: client-only knowledge
T2: authoritative readback knowledge
```

without rewriting away T1.

## Dataset

The experiment projects all **18 source trials**, not only the six case summaries.

Each XTDB `_id` is the source pre-minted Crashpoint `action_id`.

Each row also retains:

```text
trial_id
case_id
client_claim
external_outcome
observation_availability
externally_verified
effect_count
digests_match_admission
liminal_execution
liminal_response_integrity
liminal_causal_validity
liminal_continuity_posture
side_effect_committed
source_publication_commit
_valid_from
```

## Phase A — claim-only knowledge

Before authoritative readback is admitted into the projection, every action is deliberately represented as:

```text
knowledge_state = CLAIM_ONLY
external_outcome = INDETERMINATE
observation_availability = NOT_READ_BACK
externally_verified = false
effect_count = null
liminal_execution = NOT_OBSERVED
liminal_response_integrity = UNKNOWN
liminal_causal_validity = NOT_EVALUATED
liminal_continuity_posture = REVALIDATE
side_effect_committed = null
```

The client claim itself is preserved, including `SUCCESS` or `LOST`, but it is not promoted to external truth.

## Phase B — readback knowledge

The same `_id` is inserted again with the pinned published source classification.

Examples:

```text
effect_before_lost_receipt
  client_claim = LOST
  external_outcome = ONE_EFFECT_MATCHING
  externally_verified = true
  continuity = REPORT_ONLY

stopped_before_effect
  external_outcome = NO_EFFECT
  externally_verified = true
  continuity = RETRY_SIDE_EFFECT

naive_retry
  external_outcome = MULTIPLE_EFFECTS_MATCHING
  effect_count = 2
  continuity = BLOCKED

payload_mismatch
  external_outcome = ONE_EFFECT_MISMATCHED
  response_integrity = FAILED
  causal_validity = INVALID
  continuity = BLOCKED

unavailable_readback
  external_outcome = INDETERMINATE
  externally_verified = false
  continuity = REVALIDATE
```

## Required temporal invariants

For every one of the 18 actions:

1. XTDB current view must equal the pinned Phase-B / LiminalDB mapping.
2. `FOR SYSTEM_TIME AS OF <phase-A-system-time>` must return the claim-only version.
3. `FOR SYSTEM_TIME ALL FOR VALID_TIME ALL` must retain exactly two system-time versions.
4. The first version's `_system_to` must equal the second version's `_system_from`.
5. `_valid_from` must remain unchanged across both knowledge revisions.
6. `UNAVAILABLE` must remain `INDETERMINATE / REVALIDATE`.
7. XTDB must not manufacture retry permission from missing readback.

## Why this is not duplication of LiminalDB

The two systems answer different questions.

LiminalDB:

```text
What may we safely conclude or do next?
```

XTDB:

```text
What version of that knowledge did we hold at a given system time,
and what domain time did it apply to?
```

A successful experiment therefore supports this separation:

```text
Crashpoint evidence
       │
       ▼
LiminalDB safety semantics
       │
       └──────► XTDB bitemporal projection
                    │
                    ├─ current best-known view
                    ├─ prior system-time view
                    └─ complete knowledge history
```

## Execution

The dedicated workflow starts the public XTDB `v2.1.0` standalone container over pgwire, validates the pinned Crashpoint source, and runs:

```text
Phase A INSERT (18 action IDs)
        ↓
capture XTDB transaction system time
        ↓
Phase B INSERT (same 18 IDs)
        ↓
current query
        ↓
FOR SYSTEM_TIME AS OF phase-A
        ↓
FOR SYSTEM_TIME ALL + FOR VALID_TIME ALL
        ↓
offline semantic verifier
```

The workflow uses XTDB's PostgreSQL-compatible wire endpoint only. It does not modify the LiminalDB Rust core.

## What a green gate proves

Within the exact pinned source and XTDB version:

- all 18 pre-minted action IDs survive as distinct temporal entities;
- client-only knowledge is not upgraded to external truth;
- later readback creates a new system-time version rather than erasing the old one;
- the source action's valid-time anchor remains stable;
- current XTDB classifications agree with the existing LiminalDB mapping;
- unavailable readback remains indeterminate and revalidation-only.

## What it does not prove

A green gate does **not** prove:

- XTDB should replace LiminalDB storage;
- semantic equivalence between XTDB and LiminalDB;
- production distributed consistency between the two systems;
- exactly-once external effects;
- CrewAI or SafeAgent correctness;
- sudden power-loss durability;
- external trust of the Crashpoint producer;
- that XTDB itself decides whether retry is safe.

The LiminalDB continuity posture remains the safety-domain decision in this experiment.

## Canonical invariant

> **Temporal history may preserve how knowledge changed. It may not upgrade a claim into an observation or turn missing readback into permission to repeat an external effect.**
