# Start Here — LiminalDB

LiminalDB is a pre-1.0 Rust foundation for trustworthy autonomous systems: a local-first evidence and continuity database with durable replay, signed checkpoints, and an adaptive reactive runtime.

The shortest useful description is:

> LiminalDB records what was authorized, what was actually observed, whether the response was faithful, and whether an interrupted side effect may safely continue.

## 10-minute onboarding path

1. Read the root [`README.md`](../README.md) for product scope, claim boundaries, quickstart, and current evidence.
2. Read [`PRODUCT_DIRECTION_2026.md`](PRODUCT_DIRECTION_2026.md) for the active product and architecture direction.
3. Read [`BENCHMARKS.md`](BENCHMARKS.md) for measured performance evidence and caveats.
4. Read [`RELEASE_COMPATIBILITY.md`](RELEASE_COMPATIBILITY.md) for pre-1.0 compatibility expectations.
5. Build and test from the repository root:

```bash
cargo build --release -p liminal-cli
cargo test --workspace --locked
```

6. Run the local demo stack:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

7. Pick a focused issue labelled `good first issue` or `help wanted`.

## Product model

LiminalDB has four major layers:

```text
Evidence Ledger
→ Durable Storage
→ Adaptive Runtime
→ Interfaces and Federation Adapters
```

### Evidence Ledger

The trustworthy-transition ledger keeps these dimensions independent:

- authorization;
- execution observation;
- response integrity;
- causal validity;
- continuity posture.

This separation prevents a successful-looking response from becoming proof that an action was authorized, executed, causally valid, or safe to retry.

### Durable Storage

The storage layer includes:

- append-only event history;
- WAL-backed persistence;
- deterministic replay;
- snapshots;
- signed checkpoint ancestry;
- anti-rollback validation;
- process-crash recovery evidence.

### Adaptive Runtime

The biologically inspired runtime includes:

- **Cells / NodeCell** — autonomous reactive units with energy, lifecycle, and pattern affinity;
- **Impulses** — query, write, and affect signals;
- **TRS** — adaptive PID-style control;
- **Reflexes** — feedback rules reacting to runtime stress;
- **Seed Garden** — goal lifecycle: plant → grow → harvest;
- **Mirror Timeline** — replayable runtime and decision history;
- **Pattern routing** — routes impulses using affinity and runtime state.

The biological terminology is an implementation and modelling layer. It is not the product claim by itself.

## Local validation

Run commands from the repository root.

Formatting:

```bash
cargo fmt --all --check
```

Compile the complete root workspace:

```bash
cargo check --workspace --all-targets --locked
```

Run clippy:

```bash
cargo clippy --workspace --all-targets --locked
```

Run all workspace tests:

```bash
cargo test --workspace --locked --no-fail-fast
```

Run the trustworthy-transition restart example:

```bash
cargo run --locked -p liminal-store --example trustworthy_transition_ledger
```

Start the CLI/WebSocket runtime:

```bash
cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

## First interactive commands

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
:status
:mirror top 5
:snapshot
```

These commands demonstrate goal lifecycle, runtime status, replayable history, and local persistence. They are not production certification.

## Repository map

- `Cargo.toml` — canonical root workspace.
- `liminal-db/crates/liminal-core/` — adaptive runtime and domain logic.
- `liminal-db/crates/liminal-store/` — WAL, snapshots, evidence ledger, checkpoints, and durability logic.
- `liminal-db/crates/liminal-cli/` — CLI and local runtime entrypoint.
- `liminal-db/crates/liminal-bridge-net/` — network and WebSocket bridge.
- `liminal-db/crates/liminal-bridge-abi/` — ABI integration surface.
- `protocol/` — protocol definitions.
- `conformance/` — protocol conformance tests.
- `sdk/rust/` — Rust SDK and benchmark examples.
- `sdk/ts/` — TypeScript WebSocket client.
- `scripts/` — local demonstrations and utilities.
- `tools/` — evidence and CI harnesses.
- `docs/` — product, architecture, evidence, and reviewer documentation.
- `grants/` — grant-facing material.

The nested `liminal-db/Cargo.toml` covers the original runtime subset; repository-level CI must validate the canonical root workspace so protocol, SDK, and conformance members are not silently excluded.

## Safe contribution zones

Good first contributions:

- clean-machine quickstart validation;
- documentation and broken-link fixes;
- WebSocket and SDK examples;
- benchmark evidence formatting;
- demo walkthroughs;
- platform-specific troubleshooting;
- characterization tests that preserve existing behavior;
- issue and roadmap hygiene.

## Changes requiring deeper review

Discuss and isolate these before implementation:

- trustworthy-transition schema or ordering changes;
- WAL, snapshot, checkpoint, or anti-rollback changes;
- authorization or continuity semantics;
- `ClusterField` lifecycle and routing behavior;
- TRS, reflex, dream, or Seed Garden semantics;
- protocol and SDK compatibility;
- federation validation rules;
- performance and production claims;
- security or durability guarantees.

The safe decomposition sequence for `ClusterField` is documented in [`CLUSTER_FIELD_REFACTOR_PLAN.md`](CLUSTER_FIELD_REFACTOR_PLAN.md).

## Product boundary

LiminalDB currently claims:

- an executable pre-1.0 Rust implementation;
- replayable local evidence and projections;
- signed-checkpoint and process-crash test coverage;
- a measured single-node benchmark baseline with caveats;
- adapter-oriented federation planning.

It does **not** currently claim:

- drop-in replacement for mature databases or message brokers;
- exactly-once external side effects;
- sudden-power-loss durability on arbitrary hardware;
- production distributed consensus;
- production anti-rollback guarantees;
- universal causal truth;
- a completed independent production security audit;
- stable pre-1.0 APIs or enterprise compliance.

## Evidence principle

Keep these categories separate:

```text
design target != executable example != exact-head CI evidence != production guarantee
```

Good:

> `docs/BENCHMARKS.md` contains a measured single-node baseline with reproducible commands and explicit limitations.

Avoid:

> LiminalDB is universally faster or safer than mature production systems.

## Contribution protocol

A strong contribution should preserve:

1. **Clarity** — conventional engineering meaning remains visible beneath the metaphor.
2. **Reproducibility** — material claims point to commands, tests, or artifacts.
3. **Evidence discipline** — unavailable review lanes are reported as unavailable, not passed.
4. **Narrow scope** — one PR should solve one reviewable problem.
5. **Human authority** — CI and AI review may advise or block, but they do not obtain merge or deployment authority implicitly.
