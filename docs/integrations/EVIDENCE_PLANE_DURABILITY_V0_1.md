# Evidence Plane Durability v0.1

**System case:** \`EVIDENCE-PLANE-DURABILITY-001\`  
**Stacked on:** \`EVIDENCE-PLANE-001\` / PR #128  
**Upstream crash proof:** \`ContractGraph-QA@aef57d1059513b0de1c7cc5945777949175b44de\`  
**Runtime subject:** \`aegisora-ai/aegisora@2bac618215671f6f0ac8ebddf169830d4fc0f9b3\`

## Question

Can the Evidence Plane consume a freshly reproduced process-crash execution proof without weakening its uncertainty semantics?

The bridge must preserve all of these boundaries:

\`\`\`text
durable PENDING
  is not execution truth

verified NO_EFFECT
  is not retry authority

UNAVAILABLE readback
  is not NO_EFFECT

receiver Effect existence
  is not the same as an Observation being allowed to see that Effect
\`\`\`

## Source acquisition

The workflow does not download an expiring Actions artifact.

It clones the exact ContractGraph-QA proof commit, clones the exact Aegisora runtime commit, rebuilds the runtime and freshly reruns:

\`EXECUTION-RECEIPT-DURABILITY-001\`

The source proof must again establish:

- raw subject \`SIGKILL\` return code \`-9\`;
- durable PENDING independently re-read before provider dispatch;
- one provider attempt per case;
- stable receiver effect cardinality across recovery;
- \`FULL + ONE_EFFECT_MATCHING -> FINALIZE_EXISTING_NO_REDISPATCH\`;
- \`FULL + NO_EFFECT -> NO_REDISPATCH_FRESH_AUTHORIZATION_REQUIRED\`;
- \`UNAVAILABLE -> INDETERMINATE / REVALIDATE\`;
- later FULL readback may resolve the same unavailable action without another attempt.

Only after the pinned source verifier passes is a canonical durability envelope created.

## Canonical durability envelope

Each action retains:

\`\`\`text
action_id
case_id
target
payload_digest

trace_id
decision_id
execution_id
evidence_id

SIGKILL identity

durable PENDING
  record SHA-256
  fresh-process verification

receiver truth
  attempt count
  effect count
  effect rows

knowledge revisions
  observation availability
  external outcome
  verification state
  effect count
  source recovery decision

LiminalDB mapping per revision
\`\`\`

The envelope is canonical-JSON SHA-256 bound.

## LiminalDB mapping

No new core enum is introduced.

### Matching effect after crash

\`\`\`text
source:
FULL
ONE_EFFECT_MATCHING
FINALIZE_EXISTING_NO_REDISPATCH

LiminalDB:
authority = CONSUMED
execution = OBSERVED_EXECUTED
response_integrity = VERIFIED
causal_validity = VALID
continuity = REPORT_ONLY
side_effect_committed = true
\`\`\`

\`REPORT_ONLY\` is used as the existing no-redispatch final posture.

### Verified zero effect

The source proof intentionally requires **fresh authorization**.

Therefore this bridge does **not** reuse the older Crashpoint mapping:

\`\`\`text
NO_EFFECT -> RETRY_SIDE_EFFECT
\`\`\`

Instead:

\`\`\`text
authority = REVALIDATION_REQUIRED
execution = NOT_OBSERVED
response_integrity = NOT_EVALUATED
causal_validity = VALID
continuity = REVALIDATE
side_effect_committed = false
\`\`\`

The source recovery decision remains explicitly preserved as:

\`NO_REDISPATCH_FRESH_AUTHORIZATION_REQUIRED\`

### Unavailable then full readback

Revision 1:

\`\`\`text
UNAVAILABLE
INDETERMINATE
effect_count = null

authority = REVALIDATION_REQUIRED
continuity = REVALIDATE
\`\`\`

Revision 2:

\`\`\`text
FULL
ONE_EFFECT_MATCHING
effect_count = 1

authority = CONSUMED
continuity = REPORT_ONLY
\`\`\`

The native Rust test appends two continuity snapshots linked through \`previous_continuity_ref\` and verifies replay after reopening the ledger.

## XTDB projection

XTDB stores seven system-time knowledge versions:

\`\`\`text
effect_then_crash:
  PENDING_DURABLE
  READBACK_FULL

crash_before_effect:
  PENDING_DURABLE
  READBACK_FULL

unavailable_readback:
  PENDING_DURABLE
  READBACK_UNAVAILABLE
  READBACK_FULL
\`\`\`

The source durability proof does not contain authoritative world-event timestamps.

Therefore this bridge makes **no new source valid-time claim**.

All XTDB rows use the same explicit projection-only valid-time anchor:

\`2000-01-01T00:00:00Z\`

Its purpose is only to keep the system-time history deterministic. The earlier Evidence Plane / Crashpoint experiment remains the proof for genuine valid-time versus system-time semantics.

## Neo4j explanation boundary

The graph intentionally distinguishes receiver truth from available observation.

For \`unavailable_readback\`:

\`\`\`text
Action
  -> Attempt
      -> Effect                # receiver truth exists

Action
  -> Observation revision 1
      availability = UNAVAILABLE
      observed Effect edges = 0
      -> Continuity REVALIDATE

Observation revision 1
  -> NEXT_KNOWLEDGE
      -> Observation revision 2
          availability = FULL
          -> OBSERVES Effect
          -> Continuity REPORT_ONLY
\`\`\`

This demonstrates:

> existence of an Effect node is not permission to upgrade an UNAVAILABLE observation.

The graph is an explanation/provenance read model only. It does not authorize execution.

## Cross-plane invariant

\`\`\`text
Durable PENDING
+ process crash
+ unavailable evidence
        ↓
must remain
INDETERMINATE / REVALIDATE

even when receiver truth contains an effect.

Only FULL authoritative readback
may resolve the same action
without redispatch.
\`\`\`

## Claim ceiling

A green result does not prove:

- host or power-loss durability;
- distributed consensus;
- Byzantine receiver correctness;
- exactly-once external effects;
- production provider behavior;
- recipient identity;
- payment or settlement binding;
- source valid-time semantics for the durability proof.

It proves only the pinned same-host process-crash evidence path and its mapping through this Evidence Plane.
