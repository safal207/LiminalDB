# Evidence Plane v0.1

**System case:** \`EVIDENCE-PLANE-001\`  
**Status:** bounded integration experiment  
**Stacked on:** \`NEO4J-LIMINALDB-001\` / PR #125  
**Source:** pinned Crashpoint action-readback bundle from \`bb9cd47c4b0b02527aab7b369d17b32829cc4e20\`

## Question

Can one canonical evidence envelope preserve the same 18 logical actions across three different responsibilities without allowing one projection to invent authority that is absent from the source evidence?

The three responsibilities are:

\`\`\`text
LiminalDB
  What may we conclude or do next?

XTDB
  What did this projection know at a given system time,
  and what source-domain valid time did that knowledge describe?

Neo4j
  Which authorization, claim, attempt, observation and effect
  explain the continuity posture?
\`\`\`

The experiment does not make any database the source of the external fact. The pinned Crashpoint manifest remains the source fixture.

## Canonical Evidence Envelope

The workflow first converts the pinned source into one normalized envelope:

\`\`\`text
Action
  action_id
  trial_id
  case_id

Admission
  admitted_at_utc
  payload_digest
  commit_confirmed

Claim
  client value

Attempts
  attempt_id
  worker
  retry flag
  killed / exit status

Observation
  availability
  external_outcome
  externally_verified
  effect_count
  effect attempt IDs
  effect payload digests
  retained observer-ledger digest

Decision
  LiminalDB execution
  response integrity
  causal validity
  continuity posture
  side_effect_committed

Provenance
  source publication commit
  source trial ID
\`\`\`

The complete envelope is canonical-JSON hashed with SHA-256. Projection inputs for XTDB and Neo4j are regenerated from that envelope rather than independently copied from the raw source.

## LiminalDB boundary

The envelope's case semantics must be canonically equivalent to the repository's pinned \`CRASHPOINT-LIMINALDB-001\` fixture.

The existing Rust runtime test is then executed unchanged and must still prove:

- all six source case mappings;
- snapshot/reopen replay stability;
- response-integrity and causal-validity distinctions;
- duplicate-effect blocking;
- verified \`NO_EFFECT -> RETRY_SIDE_EFFECT\`;
- \`UNAVAILABLE + INDETERMINATE -> REVALIDATE\`.

The integration gate does not add new continuity authority. LiminalDB remains the decision-semantic layer.

## XTDB boundary

XTDB \`2.1.0\` receives the envelope-derived projection inputs.

The original \`XTDB-LIMINALDB-001\` invariants remain required:

- 18 current actions;
- 18 Phase-A rows;
- 36 historical system-time versions;
- exactly two system-time versions per action;
- stable valid-time anchor;
- contiguous system-time versions;
- current classifications equal the LiminalDB mapping;
- unavailable readback stays \`INDETERMINATE / REVALIDATE\`.

### Cross-axis bitemporal witness

\`clean-0\` is used as one explicit temporal witness.

Its source \`admitted_at_utc\` is the represented valid-time anchor. The workflow then queries that same valid time at two different XTDB system times:

\`\`\`text
valid time = source admission time

system time S1
  CLAIM_ONLY
  external_outcome = INDETERMINATE
  continuity = REVALIDATE

system time S2
  AUTHORITATIVE_READBACK
  external_outcome = ONE_EFFECT_MATCHING
  continuity = REPORT_ONLY
\`\`\`

The verifier requires:

\`\`\`text
source valid time < S1 < S2
\`\`\`

and requires the valid-time anchor to remain identical across both knowledge revisions.

This proves the two temporal axes are independently queryable in the projection. It does **not** claim that XTDB's system time is the original external observer's wall-clock time. It is the time this XTDB projection learned/stored the version.

## Neo4j boundary

Neo4j Community \`2026.08.1\` receives the same envelope-derived projection inputs.

The native \`NEO4J-LIMINALDB-001\` checks remain required, and the integration verifier additionally compares all 18 explanation rows back to the envelope.

The graph must preserve:

\`\`\`text
Action
  -> Claim
  -> Observation
  -> Continuity

Action
  -> Attempt
  -> Effect
\`\`\`

without collapsing duplicate attempts/effects or treating missing \`Effect\` nodes as negative evidence.

The key distinction remains:

\`\`\`text
stopped_before_effect:
  FULL + NO_EFFECT + verified
  zero Effect nodes
  RETRY_SIDE_EFFECT

unavailable_readback:
  UNAVAILABLE + INDETERMINATE
  zero Effect nodes
  REVALIDATE
\`\`\`

Therefore graph absence alone never authorizes a retry.

## Negative controls

The canonical envelope validator is deliberately run against four tampered variants. Each tampered envelope is re-hashed before verification so rejection cannot be attributed only to a broken outer SHA-256.

Required rejects:

1. **action identity vs attempt lineage**
   - mutate \`action_id\` while retaining the original attempt IDs;
   - must fail because attempt lineage no longer binds to the logical action.

2. **admission digest vs effect digest**
   - mutate the admitted payload digest while keeping a source classification that says the effect matches;
   - must fail.

3. **UNAVAILABLE -> NO_EFFECT promotion**
   - rewrite unavailable evidence as verified absence and grant retry posture;
   - must fail against the frozen source/LiminalDB semantics.

4. **retained-effect cardinality**
   - remove retained effects from a case whose authoritative readback says one effect;
   - must fail.

## Important non-test: target / recipient identity

The pinned Crashpoint fixture does not expose a source-bound external target/recipient identity suitable for independent comparison.

Therefore this experiment does **not** claim to test:

\`\`\`text
right action_id
+ right payload
+ wrong recipient
\`\`\`

A future real provider/pilot fixture with independently readable target or settlement-party identity is required for that boundary. The x402 cross-artifact settlement seam is a natural candidate once a real or consented delivery exists.

## Execution shape

\`\`\`text
Pinned Crashpoint manifest + prediction
              |
              v
raw source mapping verifier
              |
              v
Canonical Evidence Envelope
              |
      +-------+--------+
      |       |        |
      v       v        v
 LiminalDB   XTDB    Neo4j
 runtime    temporal  explanation
 replay     history   graph
      |       |        |
      +-------+--------+
              |
              v
cross-projection verifier
              |
              v
EVIDENCE-PLANE-001 PASS / FAIL
\`\`\`

All projection-specific gates are executed against the exact PR head.

## What PASS proves

Within the exact pinned fixture and database versions, a green gate proves:

- all 18 pre-minted action identities survive the canonical envelope;
- envelope semantics remain equivalent to the pinned LiminalDB mapping;
- the LiminalDB replay/runtime mapping remains valid;
- XTDB preserves claim-only and readback knowledge as distinct system-time versions;
- one explicit query demonstrates the same source valid time under two different knowledge states;
- Neo4j preserves explanation/effect lineage without promoting graph absence into absence-of-effect evidence;
- four semantic tamper controls are rejected;
- all three projections agree on the final continuity classification for the same canonical evidence.

## What PASS does not prove

A green gate does **not** prove:

- distributed atomic consistency across LiminalDB, XTDB and Neo4j;
- exactly-once external effects;
- production durability under host/power loss;
- external provider truth beyond the pinned fixture;
- target/recipient identity binding;
- payment/settlement binding;
- global causal truth;
- that XTDB or Neo4j may independently authorize execution;
- CrewAI, LangGraph, AutoGen or SafeAgent correctness outside their separately pinned experiments.

## Canonical invariant

> **Identity before execution. Evidence before conclusion. History without erasure. Explanation without inventing absence or causality.**
