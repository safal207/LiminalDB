# LiminalDB Benchmark Evidence Snapshot

**Status:** reviewer-facing evidence summary  
**Source of truth:** `docs/BENCHMARKS.md`  
**Scope:** current benchmark evidence for single-node WebSocket runtime behavior, synthetic scenario modelling, and pending reviewer-grade benchmark expansion.

This document summarizes what LiminalDB currently measures, what is still only a target or modelled scenario, and what reviewers should not infer yet.

## Executive summary

LiminalDB currently has three benchmark/evidence layers:

1. **Design targets** — performance goals stated in README.
2. **Synthetic scenario harness** — useful for scenario communication and modelling.
3. **Live benchmark runner** — measures a real LiminalDB WebSocket endpoint.

The strongest current benchmark evidence is a first verified single-node live baseline for the WebSocket runtime path.

It shows:

- live LQL round-trip latency over WebSocket,
- batch impulse ingest followed by a live LQL probe,
- estimated ingest throughput for the measured local profile,
- reproducible commands and environment details.

It does **not** yet prove production-grade performance, multi-node behavior, long-duration stability, or broad hardware claims.

## Evidence categories

| Evidence type | Status | Source |
|---|---|---|
| Design targets | Available | `README.md` |
| Synthetic scenario harness | Available | `sdk/rust/examples/iot-benchmark.rs` |
| Live benchmark runner | Available | `sdk/rust/examples/live-benchmark.rs` |
| First live benchmark baseline | Available | `docs/BENCHMARKS.md` |
| Modelled comparative use case | Available | `docs/USE_CASE_IOT_MONITORING.md` |
| Protocol conformance | Available | `conformance/` |
| Continuous performance regression checks | Pending | not yet published |
| Soak / long-duration stability | Pending | not yet published |
| Multi-node/consensus performance | Pending | not yet published |
| Snapshot + replay timing package | Pending | not yet published |

## Latest verified live baseline

Source: `docs/BENCHMARKS.md`

### Environment

- Date: `2026-04-17`
- Commit: `ad7dd5cfe91b389f9900e454972631621fc1a7be`
- OS: `Windows 10 Home`, version `2009`
- CPU: `AMD Ryzen 7 5700U with Radeon Graphics`
- RAM: `16 GB`
- Rust toolchain: `rustc 1.93.0 (254b59607 2026-01-19)`
- Benchmark binaries toolchain: `stable-x86_64-pc-windows-msvc`

### Measured profile

Benchmark profile:

```text
--warmup 50
--query-rounds 25
--batch-rounds 5
--batch-size 500
--timeline-top 20
```

Server command:

```bash
target/release/liminal-cli.exe --store .\benchmark-data --ws-port 8787
```

Runner command:

```bash
cargo run --release -p liminaldb-client --example live-benchmark -- \
  --url ws://127.0.0.1:8787 \
  --warmup 50 \
  --query-rounds 25 \
  --batch-rounds 5 \
  --batch-size 500 \
  --timeline-top 20
```

### Results

Live LQL round-trip latency:

- p50: `0.87 ms`
- p95: `1.00 ms`
- p99: `1.60 ms`
- avg: `0.95 ms`

Batch ingest + LQL probe:

- p50: `30.88 ms`
- p95: `32.68 ms`
- p99: `32.68 ms`
- avg: `32.59 ms`

Estimated ingest throughput:

- `~15.3K events/sec`

## Public validation refresh

The 2026-04-17 refresh in `docs/BENCHMARKS.md` reports improvement versus a previous published sample:

- LQL p50: `18.15 ms` -> `0.87 ms`, approximately `95.2%` improvement.
- Batch avg: `97.63 ms` -> `32.59 ms`, approximately `66.6%` improvement.
- Estimated ingest throughput: `~5.1K/sec` -> `~15.3K/sec`, approximately `3.0x` improvement.

These refresh numbers are useful for trend checking inside the repository. They should not be generalized to all hardware or workloads.

## What the current evidence supports

The current benchmark evidence supports these narrow claims:

- LiminalDB has explicit performance targets.
- LiminalDB has a synthetic benchmark harness for scenario modelling.
- LiminalDB has a live benchmark runner against a real WebSocket endpoint.
- LiminalDB has a first measured single-node baseline with environment, commit, commands, and caveats.
- LiminalDB measures live LQL round-trip latency over WebSocket in the published profile.
- LiminalDB measures batch ingest followed by a live LQL probe in the published profile.
- LiminalDB publishes protocol conformance assets separately from benchmark claims.

## What the current evidence does not prove

The current benchmark evidence does **not** prove:

- production-grade database readiness,
- universal performance across hardware,
- superiority over Postgres, Redis, Kafka, TimescaleDB, or other mature systems,
- long-duration soak stability,
- multi-node / Raft / distributed consensus performance,
- production security posture,
- production recovery guarantees,
- stable pre-1.0 protocol or API compatibility,
- broad workload behavior beyond the documented benchmark profile.

## Design targets vs measured evidence

LiminalDB README includes performance targets such as low cell-routing latency, high impulse throughput, memory goals, snapshot write goals, and recovery goals.

Those targets are useful for roadmap direction, but they must not be confused with measured benchmark evidence.

Use this rule:

```text
design target != measured benchmark != production guarantee
```

Good:

> The current live benchmark reports p95 `1.00 ms` LQL round-trip latency on one Windows machine for the documented profile.

Avoid:

> LiminalDB has proven sub-millisecond performance in production.

Good:

> Estimated ingest throughput in the published local profile is `~15.3K events/sec`.

Avoid:

> LiminalDB universally processes 15K+ events/sec across workloads.

## How to reproduce the latest verified profile

Build benchmark binaries:

```bash
cargo +stable-x86_64-pc-windows-msvc build --release \
  -p liminal-cli \
  -p liminaldb-client \
  --example live-benchmark \
  --target x86_64-pc-windows-msvc
```

Start server:

```bash
target/x86_64-pc-windows-msvc/release/liminal-cli.exe \
  --store .\benchmark-data \
  --ws-port 8787
```

Run measured profile:

```bash
target/x86_64-pc-windows-msvc/release/examples/live-benchmark.exe \
  --url ws://127.0.0.1:8787 \
  --warmup 50 \
  --query-rounds 25 \
  --batch-rounds 5 \
  --batch-size 500 \
  --timeline-top 20
```

Expected output markers:

- `Phase 1: live LQL round-trip`
- `Phase 2: ingest batch + LQL probe`
- `est ingest ... events/sec`

## Recommended next benchmark improvements

High-value next steps:

1. Publish a Linux benchmark run with host specs and commit SHA.
2. Publish a macOS benchmark run with host specs and commit SHA.
3. Add long-duration soak measurements.
4. Add memory footprint under sustained load.
5. Add snapshot + replay timing package for Mirror Timeline / persistence paths.
6. Add multi-node or consensus benchmark once the distributed layer is ready.
7. Add a CI or nightly performance regression gate.
8. Publish benchmark artifacts in a machine-readable format.
9. Compare only against external systems under carefully documented, fair benchmark conditions.

## Reviewer interpretation

The right reviewer interpretation is:

> LiminalDB has a first measured local live baseline for its WebSocket runtime path, with reproducible commands and explicit caveats.

The wrong reviewer interpretation is:

> LiminalDB has proven production-grade performance or universal superiority over mature databases.

## Evidence principle

LiminalDB benchmark language should stay tied to measured artifacts.

Good:

> In the verified local baseline, live LQL round-trip latency was p95 `1.00 ms` and p99 `1.60 ms` on the documented Windows machine.

Avoid:

> LiminalDB is production-proven at p99 `1.60 ms`.

Good:

> Reviewer-grade expansion is pending for soak, replay, multi-node, and CI performance gates.

Avoid:

> Current local benchmark evidence proves full operational readiness.
