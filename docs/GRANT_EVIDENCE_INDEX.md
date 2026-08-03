# NGI Fediversity Grant Evidence Index

Application: `2026-08-00c`

Fund: NGI Fediversity

Canonical requested amount: EUR 50,000

Canonical repository: https://github.com/safal207/LiminalDB

Review state: application acknowledged; first-round eligibility review pending.

## Reviewer thesis

LiminalDB is an executable local-first and event-sourced Rust runtime. The grant-funded transition is to turn that local evidence substrate into a clearly specified and reproducible federated replication component.

The repository does not treat federation as already complete.

## Causal and temporal transition graph

```text
application state tied to one runtime
  -> weak portability, replay, and recovery
  -> local append-only event history
  -> durable persistence and deterministic replay
  -> validated remote event envelope
  -> duplicate and conflict handling
  -> protocol adapter boundary
  -> federated state continuity with explicit evidence
```

Current verified transition:

```text
runtime events
  -> WAL / snapshots / timeline
  -> deterministic local replay
  -> inspectable CLI and WebSocket evidence
```

Grant-funded transition:

```text
local replay model
  -> mock node-to-node exchange
  -> remote validation and rejection records
  -> ActivityPub / Matrix mapping notes
  -> reproducible federated demo
```

## Submitted claim versus current evidence

| Submitted direction | Current evidence | Current state | Grant-funded delta | Acceptance test |
| --- | --- | --- | --- | --- |
| Event-sourced memory runtime | Rust workspace, event model, Mirror Timeline, CLI | Implemented | Stabilise reviewer-facing event envelope | Event format and replay assumptions are documented and testable |
| Local-first persistence | WAL, snapshots, replay-oriented storage paths | Implemented baseline | Harden pruning, compaction, and deterministic fixtures | State survives restart and projections rebuild deterministically |
| Auditable transitions | Append-only history and trustworthy-transition records | Implemented baseline | Define portable federation evidence | Rejected, duplicate, and accepted remote events remain inspectable |
| Federated replication | Architecture and milestone plan | Not yet production-implemented | Build a mock two-node exchange and validation path | Node A emits; Node B validates; duplicates are not blindly applied |
| Conflict handling | Design direction and explicit risks | Planned | Specify deterministic conflict and rejection semantics | Conflicting input produces a documented result and audit record |
| ActivityPub / Matrix integration | Mapping and adapter planning documents | Planned / documentary | Produce adapter interface and protocol mapping notes | Adapter responsibilities are separated from core runtime responsibilities |
| Privacy-aware distribution | Claim boundaries and payload-minimisation direction | Planned | Document pruning, minimisation, and encryption boundaries | Reviewer can identify what data crosses nodes and why |
| Reviewer reproducibility | Root build/test commands, demos, CI and docs | Implemented but external clean-machine validation remains open | Complete independent clean-checkout evidence | OS, Rust version, commands, and outcomes are recorded publicly |

## Reviewer commands

```bash
git clone https://github.com/safal207/LiminalDB.git
cd LiminalDB
cargo build --release -p liminal-cli
cargo test --workspace --locked
```

Local runtime:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

Expected current evidence:

```text
local event
  -> durable append
  -> timeline inspection
  -> restart / replay
  -> rebuilt state
```

Expected grant-funded federation evidence:

```text
Node A event
  -> canonical envelope
  -> Node B validation
  -> ACCEPT or REJECT
  -> duplicate detection
  -> auditable transition record
```

## User and ecosystem value

The end user should not need to understand LiminalDB internals. The intended value is indirect but concrete:

- applications can preserve local state and event history;
- service operators can inspect replication failures instead of silently losing state;
- developers can reuse a documented federation boundary rather than inventing one per application;
- users are less dependent on one central runtime for continuity and recovery.

Target ecosystems include developers working with ActivityPub, Matrix, local-first applications, federated services, event sourcing, and privacy-respecting distributed infrastructure.

## Administrative clarification

The canonical grant request is **EUR 50,000**. If an acknowledgement rendering shows an empty amount field, this document and `docs/BUDGET_AND_MILESTONES_FEDIVERSITY.md` record the intended request without changing project scope.

## Current boundaries

LiminalDB currently does not claim:

- completed production federated replication;
- production ActivityPub or Matrix adapters;
- production distributed consensus;
- universal CRDT correctness;
- independent security certification;
- production anti-rollback guarantees on arbitrary hardware.

## Decision rule

A federation milestone is complete only when:

1. input and output event states are explicit;
2. the permitted transition is deterministic;
3. duplicate, conflict, and invalid paths fail closed;
4. the result leaves reviewer-verifiable evidence;
5. the documentation distinguishes implementation from future target.
