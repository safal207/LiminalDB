# LiminalDB

![Status](https://img.shields.io/badge/status-active-brightgreen)
![Core](https://img.shields.io/badge/core-rust-blue)
![License](https://img.shields.io/badge/license-Apache--2.0-orange)

**Project status:** Active core/runtime development with CI coverage and production-focused architecture.

**Quick validation:**
```bash
cargo build --release -p liminal-cli
cargo test --workspace
```

**Single-command demo entrypoint (Windows):**
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

## Review links

- Start here: [`docs/START_HERE.md`](docs/START_HERE.md)
- 5-minute demo: [`docs/demo/FIVE_MINUTE_STACK_DEMO.md`](docs/demo/FIVE_MINUTE_STACK_DEMO.md)
- Seed Garden demo: [`docs/demo/SEED_GARDEN_DEMO.md`](docs/demo/SEED_GARDEN_DEMO.md)
- WebSocket examples: [`docs/api/WEBSOCKET_EXAMPLES.md`](docs/api/WEBSOCKET_EXAMPLES.md)
- Grant evidence: [`docs/GRANT_EVIDENCE.md`](docs/GRANT_EVIDENCE.md)
- Architecture: [`docs/`](docs/)
- Benchmark baseline: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md)
- Benchmark evidence: [`docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md`](docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md)
- Release + compatibility: [`docs/RELEASE_COMPATIBILITY.md`](docs/RELEASE_COMPATIBILITY.md)
- Validation: `cargo test --workspace`
- Security: [`SECURITY.md`](SECURITY.md)
- Roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md)
- Grant materials: [`grants/`](grants/)
- License: [`LICENSE`](LICENSE)

A biologically-inspired, self-adaptive reactive database system written in Rust.

## What is LiminalDB?

LiminalDB models data operations as a living ecosystem instead of traditional CRUD:

- **Cells** (NodeCell) — autonomous reactive agents with energy, metabolism, and pattern affinity
- **Impulses** — signals flowing through the system (Query / Write / Affect)
- **TRS** — PID control loop that automatically adapts system behaviour to changing load
- **Reflexes** — feedback rules that respond to stress signals
- **Seed Garden** — goal-oriented task lifecycle (plant → grow → harvest)
- **Mirror Timeline** — append-only epoch log for replay and audit

## Architecture

```
┌─────────────────────────────────────────┐
│           liminal-core (~9 K LOC)        │
│                                          │
│  ├─ Cell Management   (lifecycle)        │
│  ├─ Pattern Routing   (affinity index)   │
│  ├─ Adaptive Control  (TRS / PID)        │
│  ├─ Reflex System     (feedback loops)   │
│  ├─ Seed Garden       (task lifecycle)   │
│  ├─ Mirror Timeline   (event sourcing)   │
│  └─ Storage Layer     (journal + WAL)    │
└─────────────────────────────────────────┘
         ▲              ▲              ▲
   ┌─────┴────┐  ┌──────┴─────┐  ┌───┴──────┐
   │   CLI    │  │  WebSocket  │  │ WASM/ABI │
   └──────────┘  └─────────────┘  └──────────┘
```

**Design principles:** Hexagonal (Ports & Adapters), Event Sourcing, DDD bounded contexts.  
Core has zero I/O dependencies — adapters can be swapped or tested in isolation.

## Quick Start

```bash
git clone https://github.com/safal207/LiminalBD.git
cd LiminalBD

cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

Expected output:

- `ws.local_listening` in CLI logs
- `ws_server.listening addr=127.0.0.1:8787` in bridge logs
- CLI accepts commands like `:status` and `:mirror top 10`

If `liminal-cli` exits immediately on Windows, keep stdin open (for example
with `cmd /c "ping -t 127.0.0.1 | ..."`). See
[`docs/STACK_DEMO.md`](docs/STACK_DEMO.md) for a reproducible stack flow and
troubleshooting hints.

### Interactive session

```bash
:seed plant BoostToken cpu/load {"scale":0.2} 60000   # plant a goal
:seed garden                                            # inspect growth
:status                                                 # live metrics
:mirror top 5                                           # recent decisions
:snapshot                                               # persist state
```

### Rust SDK

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

### TypeScript SDK (WebSocket)

> The TypeScript SDK wraps the WebSocket protocol defined in
> [liminal-db/docs/PROTOCOL.md](liminal-db/docs/PROTOCOL.md).
> Source: `sdk/ts/src/client.ts`

```typescript
import { LiminalClient } from './sdk/ts/src/index';

const client = new LiminalClient('ws://localhost:8787');

// Subscribe to harmony events
client.on('harmony', (msg) => {
  console.log('live_load:', msg.meta?.live_load);
});

// Send an impulse via the protocol
client.send(JSON.stringify({
  cmd: 'impulse',
  pattern: 'cpu/load',
  strength: 0.8,
}));
```

## Project Status

**Version:** 0.5.x (pre-1.0, active development)

| Area | Status |
|------|--------|
| Core biological engine (cells, impulses, metabolism) | ✅ Done |
| Event sourcing (journal, snapshots, WAL) | ✅ Done |
| PID adaptive control (TRS) | ✅ Done |
| Reflex & variant decision systems | ✅ Done |
| CLI, WebSocket, WASM, FFI bridges | ✅ Done |
| Rust SDK + TypeScript SDK | ✅ Done |
| Protocol specification | ✅ Done |
| Distributed cluster / Raft consensus | 🚧 In progress |
| OpenTelemetry tracing | 🚧 In progress |
| Performance benchmarks (real hardware) | 🚧 In progress |
| Security audit | 🚧 In progress |

## Performance Targets

These are **design targets**, not yet validated measurements.
Benchmarking against a live instance is tracked in
[docs/USE_CASE_IOT_MONITORING.md](docs/USE_CASE_IOT_MONITORING.md).

| Metric | Target |
|--------|--------|
| Cell routing latency p99 | < 5 ms for 10 K cells |
| Impulse throughput | > 10 K / sec |
| Memory | < 100 MB for 10 K cells |
| Snapshot write (100 MB state) | < 500 ms |
| Recovery after node failure | < 30 s |

First measured baseline:
[`docs/BENCHMARKS.md`](docs/BENCHMARKS.md) now includes a published live
single-node benchmark sample with reproducible commands and explicit caveats.

## Use Cases

- **Adaptive monitoring** — alert thresholds that self-tune to workload
- **IoT data ingestion** — burst-tolerant pipeline without manual provisioning
- **Real-time recommendations** — pattern matching that specialises over time
- **Autonomous control** — PID-regulated decision loops with full audit trail

See [docs/USE_CASE_IOT_MONITORING.md](docs/USE_CASE_IOT_MONITORING.md) for a
detailed IoT scenario with modelled comparisons vs Redis Streams and Kafka.

## Documentation

| Document | Content |
|----------|---------|
| [docs/ARCHITECTURE_ANALYSIS.md](docs/ARCHITECTURE_ANALYSIS.md) | Design decisions, module breakdown |
| [liminal-db/docs/PROTOCOL.md](liminal-db/docs/PROTOCOL.md) | Client-server wire format |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Development roadmap |
| [docs/PHILOSOPHY.md](docs/PHILOSOPHY.md) | Intellectual and philosophical roots |
| [docs/adr/](docs/adr/) | Architecture Decision Records |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute |

## Contributing

We welcome contributions in any area — tests, docs, benchmarks, SDKs, or core
features. See [CONTRIBUTING.md](CONTRIBUTING.md) for setup instructions and
good first issues.

## License

[Apache-2.0](LICENSE) — free to use, modify, and distribute.

## Contact

safal0645@gmail.com
