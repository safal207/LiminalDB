# NGI Fediversity Reviewer Path

## Project

**LiminalDB: Federated Event-Sourced Memory Layer**

Repository: https://github.com/safal207/LiminalDB

## One-sentence summary

LiminalDB is an open-source Rust runtime for event-sourced memory, local-first state, and future federated replication across distributed services.

It turns application state into replayable, auditable event streams that can later be synchronized across federated nodes.

## Why this matters

Federated services need more than user-facing protocols.

They also need reusable infrastructure for local state, replication, replay, audit, and recovery.

Today, many Fediverse and federated-cloud applications implement their own storage, synchronization, and event history layers inside each application.

That makes new applications harder to build, harder to audit, and harder to evolve.

LiminalDB focuses on a reusable lower layer:

> local-first event memory that can be replayed, inspected, compacted, and eventually replicated across federated nodes.

## Fit with NGI Fediversity

NGI Fediversity supports open-source work for federated cloud services and a healthier decentralized internet.

LiminalDB fits this direction because it aims to provide a reusable infrastructure component for federated applications:

- local-first event-sourced state;
- append-only Mirror Timeline for replay and audit;
- WebSocket interfaces for live state observation;
- Rust core with clean architecture boundaries;
- future adapters for ActivityPub, Matrix, and related federated protocols;
- privacy-aware replication design;
- open-source documentation and reproducible demos.

LiminalDB is not a complete Fediverse application.

It is intended as a building block that can help developers create federated services with stronger state history, replay, and synchronization guarantees.

## Reviewer quick path

Start here:

1. Read this file.
2. Read `docs/FEDERATED_EVENT_SOURCING_ALIGNMENT.md`.
3. Read `docs/ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md`.
4. Run the existing validation path in the README.
5. Read `docs/BUDGET_AND_MILESTONES.md`.

## Current project status

The current repository already demonstrates:

- Rust core runtime;
- event-oriented architecture;
- Mirror Timeline append-only log;
- Seed Garden task lifecycle;
- adaptive feedback / control mechanisms;
- WebSocket-facing runtime path;
- CLI demo path;
- CI-oriented validation commands;
- documentation and benchmark evidence.

Current non-claims:

- no production federated protocol adapter yet;
- no production ActivityPub integration yet;
- no production Matrix integration yet;
- no claim of full CRDT correctness yet;
- no claim of production security or privacy guarantees before further review;
- no claim that LiminalDB replaces existing Fediverse servers or databases.

## Target outcome of the grant

The grant will turn the existing runtime into a clearer federated infrastructure component with:

- a stable event envelope format;
- a documented replication model;
- a local-first persistence and replay path;
- a federation adapter design for ActivityPub and Matrix;
- conflict-resolution and ordering notes;
- privacy-aware pruning and compaction design;
- reproducible demo scripts;
- reviewer-oriented documentation.

## Success criteria

A reviewer or developer should be able to:

- clone the repository;
- run the local runtime validation;
- inspect the Mirror Timeline event stream;
- understand how events can be replayed and audited;
- understand the proposed bridge to federated protocols;
- see a clear roadmap from current runtime to federated replication;
- reuse the architecture notes in another federated service.

## Suggested reviewer command path

```bash
git clone https://github.com/safal207/LiminalDB.git
cd LiminalDB

cargo build --release -p liminal-cli
cargo test --workspace
```

On Windows, the current demo entrypoint is:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

Expected high-level story:

```text
runtime starts
cells / impulses / seed garden are visible
mirror timeline records events
WebSocket path exposes runtime state
local demo can be replayed and inspected
```

## Grant proposal reference

Submitted application:

```text
Application 2026-08-00c — LiminalDB: Federated Event-Sourced Memory Layer
Fund: NGI Fediversity
Requested amount: EUR 50,000
Correct repository: https://github.com/safal207/LiminalDB
```
