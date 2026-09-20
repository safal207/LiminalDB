# Neo4j ↔ LiminalDB causal projection v0.1

**System case:** `NEO4J-LIMINALDB-001`  
**Status:** bounded causal read-model experiment  
**Stacked on:** `XTDB-LIMINALDB-001` / PR #123  
**Neo4j:** Community `2026.08.1`

## Purpose

This experiment tests a different question from XTDB.

XTDB answers:

```text
What did we know, and when?
```

Neo4j is tested as a causal/explanation projection:

```text
Which authorization, claim, attempt, observation and effect
support the current LiminalDB continuity posture?
```

LiminalDB remains the source of safety semantics. Neo4j does not decide whether an external side effect may be retried.

## Source

The projection uses the same pinned 18-trial Crashpoint bundle and the same `CRASHPOINT-LIMINALDB-001` mapping fixture already exercised by PR #122.

No source case is summarized away: each pre-minted `action_id` becomes its own graph root.

## Graph model

For every logical action:

```text
(:Action)
   ├─[:HAS_AUTHORIZATION]─>(:Authorization)
   ├─[:HAS_CLAIM]────────>(:Claim)
   ├─[:HAS_ATTEMPT]──────>(:Attempt)
   ├─[:HAS_OBSERVATION]──>(:Observation)
   └─[:HAS_CONTINUITY]───>(:Continuity)

(:Claim)-[:EVALUATED_AGAINST]->(:Observation)
(:Observation)-[:INFORMS]->(:Continuity)

(:Attempt)-[:PRODUCED]->(:Effect)
(:Observation)-[:OBSERVES]->(:Effect)
```

A retry can add another `Attempt` and another `Effect` without changing the logical `Action` identity.

## Expected graph cardinality

For the pinned 18-trial source:

```text
Action          18
Authorization   18
Claim           18
Observation     18
Continuity      18
Attempt         21
Effect          15
```

Why only 15 retained `Effect` nodes:

- `clean`: 3 effects;
- `effect_before_lost_receipt`: 3 effects;
- `stopped_before_effect`: 0 effects;
- `naive_retry`: 6 effects;
- `payload_mismatch`: 3 effects;
- `unavailable_readback`: no retained authoritative effect readback, so no `Effect` node is materialized.

That last distinction is deliberate.

## Critical invariant: graph absence is not negative evidence

Both of these cases have zero `Effect` nodes:

```text
stopped_before_effect
unavailable_readback
```

They must remain semantically different.

### Verified no-effect

```text
Observation.availability = FULL
Observation.external_outcome = NO_EFFECT
Observation.externally_verified = true
Observation.effect_count_known = true
Observation.effect_count = 0

Continuity.posture = RETRY_SIDE_EFFECT
```

### Unavailable readback

```text
Observation.availability = UNAVAILABLE
Observation.external_outcome = INDETERMINATE
Observation.externally_verified = false
Observation.effect_count_known = false

Continuity.posture = REVALIDATE
```

Therefore:

> The absence of an `Effect` node is never sufficient by itself to infer that no external effect happened.

## Retry multiplicity

The three `naive_retry` trials must each retain:

```text
1 Action
2 Attempts
2 Effects
1 Observation
1 Continuity(BLOCKED)
```

This prevents graph projection from collapsing duplicate effects into one successful action.

## Payload mismatch

For `payload_mismatch`:

```text
Effect.payload_matches_admission = false
Continuity.response_integrity = FAILED
Continuity.causal_validity = INVALID
Continuity.posture = BLOCKED
```

Action-ID equality is therefore not treated as evidence that the admitted payload was executed faithfully.

## Lost receipt

For `effect_before_lost_receipt`:

```text
Claim.value = LOST
Observation.external_outcome = ONE_EFFECT_MATCHING
Observation.externally_verified = true
Effect exists
Continuity.posture = REPORT_ONLY
```

The graph keeps the client claim and the external observation separate.

## Verification

The dedicated harness creates Neo4j uniqueness constraints, loads the graph, then executes named invariant checks through Cypher.

Critical checks include:

- all 18 actions have complete explanation paths;
- every retained effect has attempt lineage;
- graph effect multiplicity matches known source effect counts;
- current continuity posture matches the pinned LiminalDB mapping;
- unavailable readback stays `INDETERMINATE / REVALIDATE`;
- verified `NO_EFFECT` stays a distinct `FULL` readback condition;
- naive retries retain two attempts and two effects;
- payload mismatch stays blocked;
- lost receipts do not erase observed effects.

## Why Neo4j is not the source of truth

The graph is a projection.

LiminalDB still owns the domain rules:

```text
Authorization
Observation
Response Integrity
Causal Audit
Continuity
```

XTDB, when present, owns neither; it preserves temporal revisions of knowledge.

The intended separation is:

```text
                 LiminalDB
          safety/evidence semantics
                 /       \
                /         \
             XTDB        Neo4j
        knowledge time   causal explanation
```

## What a green gate proves

Within the exact pinned source and Neo4j version:

1. all 18 logical action identities remain distinct graph roots;
2. retry attempts/effects remain explicit rather than collapsed;
3. every retained effect can be traced to an attempt;
4. client claims remain separate from external observations;
5. the graph preserves the LiminalDB continuity mapping;
6. zero graph effects do not collapse `NO_EFFECT` and `UNAVAILABLE`;
7. causal explanation paths are queryable for every action.

## What it does not prove

A green gate does **not** prove:

- Neo4j should replace LiminalDB;
- Neo4j should decide retry safety;
- the graph is an independent authoritative external ledger;
- distributed cross-database consistency among LiminalDB, XTDB and Neo4j;
- global exactly-once effects;
- CrewAI or SafeAgent correctness;
- production graph scale or availability;
- sudden-power-loss durability;
- universal causal truth.

## Canonical invariant

> **A graph may explain the evidence chain. Missing graph edges or nodes may never be promoted into evidence that an external side effect did not occur.**
