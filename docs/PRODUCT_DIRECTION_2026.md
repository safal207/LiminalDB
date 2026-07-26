# LiminalDB Product Direction — 2026

**Status:** active direction for pre-1.0 development  
**Updated:** 2026-07-26

## Product statement

LiminalDB is a local-first evidence and continuity database for trustworthy autonomous systems.

It preserves the difference between:

1. what an actor was authorized to do;
2. what execution was actually observed;
3. whether the reported response matched the observation;
4. whether causal validity was evaluated;
5. whether an interrupted side effect may continue, retry, stop, or require revalidation.

The product is not positioned as a drop-in replacement for Postgres, Redis, Kafka, or a vector database. Its differentiator is durable, replayable evidence around autonomous actions and recovery decisions.

## Product architecture

```text
LiminalDB
├── Evidence Ledger
│   ├── Authorization
│   ├── Observation
│   ├── Response Integrity
│   ├── Causal Audit
│   └── Continuity
│
├── Durable Storage
│   ├── WAL
│   ├── Snapshots
│   ├── Hash-Chained Events
│   ├── Signed Checkpoints
│   └── Anti-Rollback Anchors
│
├── Adaptive Runtime
│   ├── Cells and Pattern Routing
│   ├── TRS Adaptive Control
│   ├── Reflexes
│   ├── Mirror Timeline
│   └── Seed Garden
│
└── Adapters
    ├── CLI
    ├── WebSocket
    ├── Rust and TypeScript SDKs
    ├── ABI / WASM
    └── Federation Adapters
```

## Primary use cases

### 1. AI-agent side-effect continuity

An agent submits a payment, reservation, deployment, email, or other external action. The process times out or restarts before it can determine the result.

LiminalDB stores sufficient independent evidence to decide whether the action should be retried, reported only, blocked, or revalidated.

### 2. Tool-response integrity

A tool or agent reports that an operation succeeded. LiminalDB preserves the execution observation independently from the response and records whether the response was verified, partial, failed, or not evaluated.

### 3. Local-first audit and replay

A service records evidence locally and can rebuild the same projection after restart. Future federation adapters may exchange validated event envelopes without making the core depend on a specific protocol.

### 4. Safety rehearsal and verified-negative memory

A system can preserve evidence that a candidate memory or action was explicitly **not** accepted for production use. This prevents a report-only or test-only result from silently becoming durable operational authority.

## Product invariants

LiminalDB development must preserve these rules:

1. **Authorization is not execution.**
2. **Execution is not a successful response.**
3. **Response integrity is not causal truth.**
4. **A valid signature is not production authority.**
5. **A process-crash test is not a sudden-power-loss guarantee.**
6. **Replay must be deterministic for the same accepted event sequence.**
7. **A partial projection must not be accepted as a complete transition.**
8. **Authority to store evidence is not authority to repeat an external side effect.**
9. **Head movement invalidates exact-head review evidence.**
10. **Design targets, measured evidence, and production guarantees remain separate.**

## 2026 priorities

### Priority A — Product clarity and reviewer path

Deliverables:

- one product statement across README and onboarding;
- one architecture diagram and terminology set;
- a five-minute evidence-ledger demonstration;
- explicit claim boundaries;
- direct links to measured and CI evidence.

Exit criteria:

- a new reviewer can explain LiminalDB in one sentence;
- the quickstart runs from the repository root;
- product claims can be mapped to code, tests, or a clearly labelled roadmap item.

### Priority B — Evidence-ledger developer API

Deliverables:

- stable pre-1.0 Rust API for transition events and projections;
- documented JSON/event-envelope schemas;
- import/export fixtures;
- idempotent record ownership and duplicate handling;
- examples for interrupted side effects and verified-negative memory.

Exit criteria:

- an external developer can record and replay one complete transition without using internal crate details;
- invalid ordering and cross-transition references fail closed;
- compatibility expectations are documented.

### Priority C — Durability evidence

Deliverables:

- full workspace CI;
- cross-platform safety and process-crash matrices;
- snapshot/replay timing pack;
- long-duration soak profile;
- published artifact-retention policy;
- explicit unsupported-filesystem and power-loss boundaries.

Exit criteria:

- every durability claim points to a reproducible command or retained artifact;
- recovery never accepts a partial transition projection;
- performance regression monitoring exists without pretending GitHub-hosted runners are stable benchmark hardware.

### Priority D — Architectural decomposition

Deliverables:

- split `ClusterField` behind a compatibility-preserving facade;
- isolate registry, routing, adaptive control, timeline, and goal-runtime responsibilities;
- add focused unit tests around extracted components;
- remove outdated architectural metrics and duplicate workspaces where practical.

Exit criteria:

- `ClusterField` no longer owns every subsystem directly;
- extracted modules can be tested without constructing the entire runtime;
- public behavior and serialized formats remain unchanged during the first refactor phase.

See [`CLUSTER_FIELD_REFACTOR_PLAN.md`](CLUSTER_FIELD_REFACTOR_PLAN.md).

### Priority E — Federation as adapters, not core coupling

Deliverables:

- stable event-envelope mapping;
- duplicate and remote-event validation rules;
- rejection records for invalid events;
- mock ActivityPub and Matrix adapter demonstrations;
- payload minimization and privacy notes.

Exit criteria:

- the local evidence ledger works without federation;
- remote events cannot bypass local validation;
- protocol-specific concepts do not leak into core domain types.

## Non-goals for the current phase

- claiming universal causal truth;
- replacing mature general-purpose databases;
- promising exactly-once external side effects;
- production Raft or Byzantine consensus claims;
- automatic execution, deployment, submission, or merge authority;
- hiding test-only keys or controlled anchors behind production terminology;
- expanding the biological metaphor where a conventional engineering term is clearer.

## Evidence ladder

```text
F0 — idea or design target
F1 — executable example
F2 — deterministic unit/integration test
F3 — exact-head CI evidence
F4 — cross-platform or independent reproduction
F5 — production observation under declared operating conditions
```

Documentation must state which level supports each material claim.

## Milestone sequence

1. Product and CI hardening.
2. Merge-ready evidence-ledger examples and recovery fixtures.
3. Public developer API and schema documentation.
4. Safe `ClusterField` decomposition.
5. Soak and snapshot/replay benchmark pack.
6. Mock federation adapters.
7. External security and durability review.

Each milestone should be independently reviewable and should not obtain merge, deployment, or production authority from another milestone implicitly.
