# LiminalDB

[![LiminalDB CI](https://github.com/safal207/LiminalDB/actions/workflows/ci.yml/badge.svg)](https://github.com/safal207/LiminalDB/actions/workflows/ci.yml)
[![Security Baseline](https://github.com/safal207/LiminalDB/actions/workflows/security.yml/badge.svg)](https://github.com/safal207/LiminalDB/actions/workflows/security.yml)
![Core](https://img.shields.io/badge/core-Rust-blue)
![License](https://img.shields.io/badge/license-Apache--2.0-orange)
![Status](https://img.shields.io/badge/status-pre--1.0-active-brightgreen)

**A local-first evidence and continuity database for trustworthy autonomous systems.**

LiminalDB records five questions that ordinary logs and conversation memory usually collapse together:

1. **What was authorized?**
2. **What was actually observed?**
3. **Was the reported response faithful to the observation?**
4. **Was the causal interpretation evaluated?**
5. **May an interrupted side effect safely continue, retry, stop, or require revalidation?**

It combines an append-only trustworthy-transition ledger, WAL-backed durable storage, replayable projections, signed checkpoints, anti-rollback checks, and an adaptive reactive runtime written in Rust.

```text
authorization
→ execution observation
→ response-integrity verdict
→ causal-audit state
→ durable continuity decision
```

> LiminalDB is pre-1.0 research and infrastructure. It does not claim production database certification, production distributed consensus, universal performance superiority, or a completed independent security audit.

## Why LiminalDB exists

AI agents and automated services can fail in a dangerous gap between **intent**, **execution**, and **reported outcome**. A process can time out after committing a side effect, restart with partial state, or report success without sufficient evidence.

LiminalDB keeps these dimensions independent and replayable instead of reducing them to one ambiguous status field.

## Core capabilities

### Evidence Ledger

- immutable authorization, observation, response-integrity, causal-audit, and continuity records;
- canonical payload digests and record references;
- deterministic projection rebuild after restart;
- append-only event hash chaining;
- explicit authority and side-effect boundaries.

### Durable Storage

- WAL and snapshots;
- file synchronization before acknowledgement;
- signed checkpoint ancestry;
- trusted-key lifecycle and revocation rules;
- external anti-rollback anchor interface;
- crash-consistency evidence on Ubuntu, Windows, and macOS.

### Adaptive Runtime

- **Cells** — autonomous reactive units with energy, lifecycle, and pattern affinity;
- **Impulses** — query, write, and affect signals;
- **TRS** — PID-style adaptive control;
- **Reflexes** — feedback rules responding to runtime stress;
- **Seed Garden** — goal lifecycle: plant → grow → harvest;
- **Mirror Timeline** — replayable event and decision history.

### Interfaces

- Rust workspace and SDK;
- CLI runtime;
- WebSocket protocol and TypeScript client;
- WASM/ABI and network adapters;
- protocol conformance suite.

## Quick validation

Run from the repository root:

```bash
git clone https://github.com/safal207/LiminalDB.git
cd LiminalDB
cargo build --release -p liminal-cli
cargo test --workspace --locked
```

Start the local runtime:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

Windows demo:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

Expected success markers:

```text
ws.local_listening
ws_server.listening addr=127.0.0.1:8787
```

Interactive commands:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
:status
:mirror top 5
:snapshot
```

## Rust example

```rust
use liminal_core::*;

let mut field = ClusterField::new();
field.spawn_cell_with_pattern("cpu/load");
field.ingest_impulse(Impulse::query("cpu/load", 0.8));
field.tick_all();

for event in field.drain_events() {
    println!("{:?}", event);
}
```

## TypeScript WebSocket example

```typescript
import { LiminalClient } from './sdk/ts/src/index';

const client = new LiminalClient('ws://localhost:8787');

client.on('harmony', (message) => {
  console.log('live_load:', message.meta?.live_load);
});

client.send(JSON.stringify({
  cmd: 'impulse',
  pattern: 'cpu/load',
  strength: 0.8,
}));
```

## Architecture

```text
LiminalDB
├── Evidence Ledger
│   ├── Authorization
│   ├── Observation
│   ├── Response Integrity
│   ├── Causal Audit
│   └── Continuity
├── Durable Storage
│   ├── WAL
│   ├── Snapshots
│   ├── Signed Checkpoints
│   └── Anti-Rollback
├── Adaptive Runtime
│   ├── Cells and Routing
│   ├── TRS and Reflexes
│   ├── Mirror Timeline
│   └── Seed Garden
└── Adapters
    ├── CLI and WebSocket
    ├── SDKs and ABI
    └── Future Federation
```

The core follows ports-and-adapters, event-sourcing, and bounded-context principles. Current architectural debt and the safe `ClusterField` decomposition sequence are documented in [`docs/CLUSTER_FIELD_REFACTOR_PLAN.md`](docs/CLUSTER_FIELD_REFACTOR_PLAN.md).

## Evidence status

**Measured evidence is kept separate from design targets and production guarantees.**

The published single-node WebSocket baseline currently reports:

- LQL round-trip p50: `0.87 ms`;
- LQL round-trip p99: `1.60 ms`;
- estimated ingest throughput: approximately `15.3K events/sec`.

These measurements come from one documented developer machine and are not universal performance claims. See [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) for commands, hardware, results, and caveats.

Pending evidence includes long-duration soak tests, replay timing packs, multi-node performance, and continuous performance regression checks.

## Current status

| Area | Status |
|---|---|
| Adaptive runtime core | ✅ Implemented |
| Event sourcing, WAL, snapshots, replay | ✅ Implemented |
| Trustworthy-transition ledger | ✅ Implemented |
| Signed checkpoints and anti-rollback boundary | ✅ Implemented |
| Cross-platform crash-consistency matrix | ✅ Implemented |
| CLI, WebSocket, Rust and TypeScript surfaces | ✅ Implemented |
| First measured single-node baseline | ✅ Published |
| Federated replication | 🚧 Design / implementation work |
| OpenTelemetry tracing | 🚧 In progress |
| Long-duration and multi-node benchmarks | 🚧 Pending |
| Independent production security audit | 🚧 Pending |

## Product and roadmap

- Current product direction: [`docs/PRODUCT_DIRECTION_2026.md`](docs/PRODUCT_DIRECTION_2026.md)
- Contributor onboarding: [`docs/START_HERE.md`](docs/START_HERE.md)
- Architecture analysis: [`docs/ARCHITECTURE_ANALYSIS.md`](docs/ARCHITECTURE_ANALYSIS.md)
- Historical roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md)
- Benchmark evidence: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md)
- Release compatibility: [`docs/RELEASE_COMPATIBILITY.md`](docs/RELEASE_COMPATIBILITY.md)
- Security policy: [`SECURITY.md`](SECURITY.md)

## NGI Fediversity reviewer path

LiminalDB was submitted to NGI Fediversity as an open-source local-first and federated event-sourced memory layer.

Reviewer path:

1. Read [`docs/FEDIVERSITY_REVIEWER_PATH.md`](docs/FEDIVERSITY_REVIEWER_PATH.md).
2. Run `cargo build --release -p liminal-cli`.
3. Run `cargo test --workspace --locked`.
4. Run the Windows demo when applicable.
5. Compare claims with [`READY_FOR_REVIEW.md`](READY_FOR_REVIEW.md), [`docs/GRANT_EVIDENCE.md`](docs/GRANT_EVIDENCE.md), and [`docs/BUDGET_AND_MILESTONES_FEDIVERSITY.md`](docs/BUDGET_AND_MILESTONES_FEDIVERSITY.md).

```text
Application: 2026-08-00c
Fund: NGI Fediversity
Requested amount: EUR 50,000
Repository: https://github.com/safal207/LiminalDB
```

Federation planning:

- [`docs/FEDERATED_EVENT_SOURCING_ALIGNMENT.md`](docs/FEDERATED_EVENT_SOURCING_ALIGNMENT.md)
- [`docs/ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md`](docs/ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md)
- [`docs/GRANT_MILESTONE_TRACKER_FEDIVERSITY.md`](docs/GRANT_MILESTONE_TRACKER_FEDIVERSITY.md)

## Claim boundary

Safe statements:

- the repository contains an executable Rust implementation;
- durable transition state can be persisted and deterministically replayed;
- signed-checkpoint, safety, and process-crash cases have reproducible CI evidence;
- a measured single-node baseline is published with explicit caveats.

Not currently claimed:

- sudden-power-loss durability on arbitrary hardware;
- hostile or network-filesystem correctness;
- production anti-rollback guarantees;
- production-grade distributed consensus;
- universal causal truth;
- stable pre-1.0 APIs;
- enterprise certification or compliance.

## License

Apache License 2.0. See [`LICENSE`](LICENSE).
