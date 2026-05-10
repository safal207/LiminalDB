# Start Here — Contributing to LiminalDB

LiminalDB is a Rust-based, biologically inspired reactive database system. It models data and runtime behavior as a living ecosystem of cells, impulses, adaptive control loops, reflexes, goals, and replayable timelines.

The short version:

> LiminalDB is an adaptive reactive database where data operations behave less like static CRUD and more like a living, observable, self-adjusting system.

## 10-minute onboarding path

1. Read the root `README.md` for the project story, quickstart, architecture, and status.
2. Read `docs/ARCHITECTURE_ANALYSIS.md` for system structure and design decisions.
3. Read `docs/BENCHMARKS.md` for measured benchmark evidence and caveats.
4. Read `docs/RELEASE_COMPATIBILITY.md` to understand version and compatibility expectations.
5. Build and test the workspace locally:

```bash
cargo build --release -p liminal-cli
cargo test --workspace
```

6. Try the local demo stack:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

7. Pick an issue labeled `good first issue` or `help wanted`.

## What LiminalDB is

LiminalDB is a pre-1.0 Rust database/runtime experiment focused on adaptive state, reactive control, and replayable system behavior.

Its core idea is that a database can do more than store records. It can observe signals, route impulses, adjust behavior, record decisions, and expose the history of adaptation.

## Core concepts

- **Cells / NodeCell** — autonomous reactive units with energy, metabolism, lifecycle, and pattern affinity.
- **Impulses** — signals flowing through the system, such as `Query`, `Write`, or `Affect`.
- **TRS** — an adaptive PID-style control loop that helps the system respond to load and stress.
- **Reflexes** — feedback rules that react to stress signals and runtime conditions.
- **Seed Garden** — goal-oriented task lifecycle: plant -> grow -> harvest.
- **Mirror Timeline** — append-only event/timeline layer for replay, audit, and inspection.
- **Pattern routing** — routes impulses based on affinity and runtime state.
- **Adapters** — CLI, WebSocket, WASM/ABI, and SDK layers around the core.

## Local validation

Build the CLI:

```bash
cargo build --release -p liminal-cli
```

Run the full workspace tests:

```bash
cargo test --workspace
```

Run with debug logs if needed:

```bash
RUST_LOG=debug cargo test --workspace
```

Start a local CLI/WebSocket instance:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

On Windows, use the provided demo stack script:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

## First interactive commands

After starting `liminal-cli`, try:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
:status
:mirror top 5
:snapshot
```

What these commands show:

- `:seed plant ...` creates a goal in the Seed Garden.
- `:seed garden` inspects active goal lifecycle state.
- `:status` shows live system/runtime status.
- `:mirror top 5` inspects recent Mirror Timeline events.
- `:snapshot` persists state.

## Safe contribution zones

These are good places for new contributors:

- Documentation improvements.
- Clean-machine quickstart validation.
- Demo walkthroughs.
- WebSocket protocol examples.
- TypeScript SDK examples.
- Benchmark evidence summaries.
- README visual polish.
- Seed Garden documentation.
- Mirror Timeline examples.
- Colab / Python binding docs.
- Platform-specific troubleshooting notes.

## Changes that need deeper review

Discuss these before implementation:

- Core cell lifecycle changes.
- Metabolism or energy model changes.
- TRS/PID control behavior changes.
- Reflex rule semantics.
- Mirror Timeline/event sourcing format changes.
- Journal/WAL/snapshot compatibility changes.
- WebSocket protocol changes.
- SDK API compatibility changes.
- WASM/ABI/FFI surface changes.
- Distributed cluster or Raft changes.
- Benchmark claims or production performance claims.
- Security, persistence, or compatibility guarantees.

## Repository map

- `liminal-core/` — core biological engine and domain logic.
- `liminal-db/` — database/protocol-related components and docs.
- `liminal-cli/` — CLI and local runtime entrypoint.
- `sdk/ts/` — TypeScript SDK / WebSocket client surface.
- `bindings/` — language bindings, including Python-related work.
- `scripts/` — local demo and utility scripts.
- `docs/` — architecture, roadmap, benchmark, and usage documentation.
- `grants/` — grant-facing materials.

Exact layout may evolve while the project is pre-1.0, so check the current tree before making broad changes.

## Product boundary

LiminalDB is currently best described as:

> a pre-1.0 Rust foundation for adaptive reactive storage and replayable runtime behavior.

It does **not** currently claim:

- production database certification,
- drop-in replacement for Postgres, Redis, Kafka, or other mature systems,
- verified performance across all workloads and hardware,
- production-grade distributed consensus guarantees,
- production security audit completion,
- stable pre-1.0 API or protocol compatibility,
- mature enterprise compliance guarantees.

## Evidence principle

LiminalDB documentation should keep these categories separate:

```text
design target != measured benchmark != production guarantee
```

Good:

> `docs/BENCHMARKS.md` contains a measured single-node baseline with reproducible commands and explicit caveats.

Avoid:

> LiminalDB is universally faster than mature production databases.

## Recommended first issues

Good starting points:

1. Verify the quickstart on a clean machine.
2. Add a 5-minute stack demo walkthrough.
3. Add a benchmark evidence snapshot for reviewers.
4. Add WebSocket protocol examples.
5. Add TypeScript SDK smoke example.
6. Add Seed Garden demo walkthrough.
7. Add README benchmark/status badges.

## Contribution principle

A strong LiminalDB contribution should preserve three things:

1. **Clarity** — readers should understand the living-system metaphor without losing the concrete Rust/runtime model.
2. **Reproducibility** — demos and benchmarks should be runnable locally.
3. **Evidence discipline** — performance and production claims should stay tied to measured artifacts and caveats.
