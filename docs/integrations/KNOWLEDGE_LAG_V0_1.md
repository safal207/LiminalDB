# KNOWLEDGE-LAG-001

**Architecture line:** Multi-Store Truth / Evidence Plane  
**Stacked on:** EVIDENCE-PLANE-DURABILITY-001 / PR #129

## Research question

Can one external effect be valid in the world before the system knows enough to conclude that it happened, while every store preserves its own authority boundary?

The experiment distinguishes:

\`\`\`text
what happened
≠
what the runtime claimed
≠
what the observer could verify
≠
what the knowledge store knew at a given system-time
≠
what the policy/causal layer was allowed to conclude
\`\`\`

## Source authority domains

The bounded source fixture uses three independent SQLite stores.

### Admission/control authority

Owns:

- \`action_id\`;
- target;
- payload digest;
- admission time;
- dispatch start time;
- consumed dispatch authority;
- runtime claim.

It does not establish external consequence truth.

### Receiver/consequence authority

Owns:

- provider attempt;
- external effect record;
- effect identity;
- **effect commit time T1**.

The effect timestamp is assigned here, before any XTDB projection exists.

### Observer authority

Owns readback observations.

Revision 1:

\`\`\`text
T3
availability = UNAVAILABLE
outcome = INDETERMINATE
effect_count = null
observed_effect_id = null
\`\`\`

The T3 code path intentionally does not open the receiver DB.

Revision 2:

\`\`\`text
T4
availability = FULL
outcome = ONE_EFFECT_MATCHING
effect_count = 1
observed_effect_id = receiver effect id
observed_effect_committed_at = T1
\`\`\`

## Source timeline

A source-local UTC clock with a monotonic floor produces strictly ordered timestamps:

\`\`\`text
T0   action admitted
T0D  dispatch starts
T1   receiver commits effect
T2   runtime records SUCCESS claim
T3   observer readback unavailable
T4   observer reads the T1 effect
\`\`\`

Required:

\`T0 < T0D < T1 < T2 < T3 < T4\`

The source verifier independently reopens all three SQLite databases and checks the timeline and bindings.

## Live XTDB ingestion

This is not an offline reconstruction.

The workflow interleaves source events and XTDB transactions:

\`\`\`text
T1 effect exists
T2 SUCCESS claim
    ↓
XTDB insert CLAIM_ONLY
    ↓
capture system-time S2

T3 observer says UNAVAILABLE
    ↓
XTDB insert READBACK_UNAVAILABLE
    ↓
capture system-time S3
    ↓
query valid-time T1 as-of S3
    => INDETERMINATE / REVALIDATE

only after that query:

T4 observer obtains FULL readback
    ↓
XTDB insert READBACK_FULL
    ↓
capture system-time S4
\`\`\`

After S4, the workflow queries T1 **again AS OF S3**.

Required:

\`\`\`text
valid-time T1 + system-time S3
  = READBACK_UNAVAILABLE / INDETERMINATE / REVALIDATE
\`\`\`

even though the current XTDB row is now:

\`\`\`text
valid-time T1 + system-time S4
  = READBACK_FULL / ONE_EFFECT_MATCHING / REPORT_ONLY
\`\`\`

This is the core bitemporal statement:

> Later evidence can change what the system currently knows about T1 without rewriting what the system knew before that evidence arrived.

## Important clock distinction

Three time domains remain separate:

1. **receiver valid-time**
   - source authority timestamp for the effect;
   - T1.

2. **source observer time**
   - T2/T3/T4 timestamps recorded by the bounded source fixture.

3. **XTDB system-time**
   - transaction time assigned by XTDB when the corresponding knowledge revision is inserted.

The experiment does not pretend that XTDB system-time equals the source observer timestamp. It proves that the system-time versions preserve the same live knowledge ordering.

## LiminalDB mapping

No new core enums are required.

### T2 claim only

\`\`\`text
authority = CONSUMED
execution = NOT_OBSERVED
response_integrity = UNKNOWN
causal_validity = NOT_EVALUATED
continuity = REVALIDATE
side_effect_committed = unknown
\`\`\`

### T3 unavailable readback

\`\`\`text
authority = CONSUMED
execution = NOT_OBSERVED
response_integrity = UNKNOWN
causal_validity = NOT_EVALUATED
continuity = REVALIDATE
side_effect_committed = unknown
\`\`\`

The new T3 Observation invalidates the previous current response/causal/continuity derivation. A fresh derivation is produced from the enlarged observation set.

### T4 full readback

\`\`\`text
authority = CONSUMED
execution = OBSERVED_EXECUTED
response_integrity = VERIFIED
causal_validity = VALID
continuity = REPORT_ONLY
side_effect_committed = true
\`\`\`

The T4 Observation again invalidates the previous current derivation and creates a new conclusion over the complete observation set.

## Neo4j explanation projection

Neo4j is a read-only provenance view, not an oracle.

The final graph may contain the receiver Effect node because the final evidence bundle knows that the effect existed.

But the T3 observation remains explicitly disconnected from it:

\`\`\`text
Action
  └─ HAS_RECEIVER_EFFECT ─> Effect {committed_at: T1}

Observation T3 {UNAVAILABLE}
  ├─ OBSERVES Effect edges = 0
  └─ INFORMS -> Continuity {REVALIDATE}

Observation T4 {FULL}
  ├─ OBSERVES -> Effect
  └─ INFORMS -> Continuity {REPORT_ONLY}
\`\`\`

The graph therefore preserves:

> world truth may exist without being available to a particular observation.

## Canonical laws exercised

### Projection non-escalation

> A projection may organize evidence, but it may not upgrade evidence.

### Historical knowability

> Later knowledge may change the current conclusion, but it must not rewrite what was knowable earlier.

### Authority provenance

> No storage engine creates authority that the underlying evidence source did not possess.

## What this experiment adds beyond PR #129

PR #129 proved:

- durable PENDING;
- real process SIGKILL;
- unavailable evidence remains INDETERMINATE;
- later FULL readback can safely finalize the same action;
- LiminalDB / XTDB / Neo4j preserve the recovery boundary.

KNOWLEDGE-LAG-001 adds a new temporal fact:

> the receiver effect has its own source-authoritative valid-time T1, and XTDB learns the truth about that same T1 only in a later system-time version.

That is the new information.

## Claim ceiling

This is a bounded same-host fixture.

It does not prove:

- globally synchronized clocks;
- distributed transaction ordering;
- Byzantine source correctness;
- production-provider semantics;
- exactly-once effects;
- recipient identity;
- payment or settlement binding.

It proves separation between source effect valid-time and live knowledge-over-time for one controlled action.
