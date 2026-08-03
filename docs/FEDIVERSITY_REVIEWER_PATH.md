# NGI Fediversity Reviewer Path

## Project

**LiminalDB: Federated Event-Sourced Memory Layer**

Repository: https://github.com/safal207/LiminalDB

Application: `2026-08-00c`

Canonical requested amount: EUR 50,000

## One-sentence summary

LiminalDB is an open-source Rust runtime for event-sourced memory and local-first state. The grant-funded transition is a reproducible federated replication model with explicit validation, conflict handling, rejection evidence, and protocol adapter boundaries.

## Claim boundary

LiminalDB already provides an executable local runtime, durable event history, replay paths, CLI and WebSocket interfaces.

It does **not** currently claim completed production federated replication or production ActivityPub / Matrix adapters.

```text
implemented now:
  local event-sourced runtime + persistence + replay + inspectable evidence

grant-funded next:
  remote event envelopes + duplicate/conflict handling + adapter mappings
```

## Why this matters

Federated services need more than user-facing protocols.

They need reusable infrastructure for:

- local state continuity;
- durable event history;
- replay and recovery;
- validated remote events;
- explicit duplicate and conflict handling;
- auditable rejection records;
- protocol adapter boundaries.

Without these properties, every application must invent its own replication and recovery model, and failures may become silent state loss.

## Fit with NGI Fediversity

LiminalDB is intended as an open infrastructure building block for federated and local-first services.

The project contributes:

- an executable Rust event-sourced runtime;
- local persistence, snapshots, and replay;
- append-only transition evidence;
- a documented path to remote event validation;
- planned ActivityPub and Matrix mappings;
- privacy-aware payload and pruning boundaries;
- reproducible demos and reviewer commands.

LiminalDB is not a complete Fediverse application and does not replace existing servers or databases.

## Reviewer quick path

1. Read [`GRANT_EVIDENCE_INDEX.md`](GRANT_EVIDENCE_INDEX.md).
2. Read this file.
3. Read [`FEDERATED_EVENT_SOURCING_ALIGNMENT.md`](FEDERATED_EVENT_SOURCING_ALIGNMENT.md).
4. Read [`ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md`](ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md).
5. Run the root build and test commands below.
6. Read [`BUDGET_AND_MILESTONES_FEDIVERSITY.md`](BUDGET_AND_MILESTONES_FEDIVERSITY.md).
7. Inspect [`GRANT_MILESTONE_TRACKER_FEDIVERSITY.md`](GRANT_MILESTONE_TRACKER_FEDIVERSITY.md).

## Current repository evidence

The repository currently demonstrates:

- Rust core runtime and SDK surfaces;
- event-oriented architecture;
- WAL, snapshots, and replay-oriented storage;
- Mirror Timeline append-only history;
- trustworthy-transition and signed-checkpoint work;
- CLI and WebSocket runtime paths;
- cross-platform crash-consistency evidence;
- benchmark documentation with explicit caveats;
- CI-oriented validation commands;
- Fediversity milestones and reviewer documentation.

Current non-claims:

- no completed production federated replication;
- no production ActivityPub adapter;
- no production Matrix adapter;
- no production distributed consensus claim;
- no universal CRDT correctness claim;
- no independent production security certification.

## Reviewer command path

```bash
git clone https://github.com/safal207/LiminalDB.git
cd LiminalDB
cargo build --release -p liminal-cli
cargo test --workspace --locked
```

Optional quality checks:

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked
```

Local runtime:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

Windows demo:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

Expected current story:

```text
local event
  -> durable append
  -> timeline inspection
  -> restart and replay
  -> reconstructed state
```

## Target grant transition

```text
local replay model
  -> canonical remote event envelope
  -> mock Node A / Node B exchange
  -> duplicate and invalid-event rejection
  -> auditable conflict outcome
  -> ActivityPub / Matrix adapter mapping
```

The grant will produce:

- a stable event envelope draft;
- local-first persistence and replay documentation;
- deterministic test fixtures;
- a mock node-to-node validation path;
- duplicate detection and rejection records;
- conflict-handling notes;
- ActivityPub and Matrix mapping notes;
- adapter interface and privacy boundaries;
- reviewer demos and final report.

## Success criteria

A reviewer should be able to:

- build and test the current local runtime;
- inspect local event history and replay behavior;
- distinguish current implementation from future federation work;
- run a grant-funded two-node mock exchange;
- observe accepted, duplicate, conflicting, and invalid remote events;
- verify that every transition leaves an auditable record;
- understand where protocol adapters connect without coupling them to the core runtime.

## Administrative clarification

The canonical request is **EUR 50,000**. If the acknowledgement email renders the amount field as empty, the requested amount is recorded here and in `BUDGET_AND_MILESTONES_FEDIVERSITY.md`; this does not change project scope.

## Grant proposal reference

```text
Application: 2026-08-00c
Fund: NGI Fediversity
Requested amount: EUR 50,000
Repository: https://github.com/safal207/LiminalDB
```
