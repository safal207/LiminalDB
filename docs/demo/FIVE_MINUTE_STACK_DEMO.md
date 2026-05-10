# Five-Minute LiminalDB Stack Demo

**Goal:** see LiminalDB's core runtime idea quickly: start the local CLI/WebSocket runtime, plant a Seed Garden goal, inspect live state, and review recent events through Mirror Timeline.

This is a local reviewer/contributor demo. It is not a production deployment guide and does not make production performance claims.

## What you will see

In five minutes, you should be able to:

1. build `liminal-cli`,
2. start a local LiminalDB runtime,
3. plant a Seed Garden goal,
4. inspect the garden state,
5. inspect runtime status,
6. inspect recent Mirror Timeline events,
7. understand how cells, impulses, TRS, reflexes, Seed Garden, and Mirror Timeline fit together.

## Core idea

LiminalDB models database/runtime behavior as a living adaptive system.

```text
impulse -> pattern routing -> cell state -> adaptive control / reflexes -> timeline event
```

The demo uses three visible surfaces:

```text
CLI commands -> live runtime state -> Mirror Timeline inspection
```

## Prerequisites

- Rust toolchain installed.
- Repository cloned locally.
- Run commands from the repository root.

```bash
git clone https://github.com/safal207/LiminalBD.git
cd LiminalBD
```

## Step 1 — Build the CLI

```bash
cargo build --release -p liminal-cli
```

Expected binary:

```text
./target/release/liminal-cli
```

On Windows, the binary may be under a target-specific release directory depending on your toolchain setup.

## Step 2 — Start the local runtime

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

Expected signs of success:

- CLI stays open and accepts commands,
- WebSocket endpoint is available on `ws://127.0.0.1:8787`,
- logs mention local WebSocket listening, such as `ws.local_listening` or `ws_server.listening`.

### Windows helper

The repository includes a single-command demo entrypoint for Windows:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

If `liminal-cli` exits immediately on Windows, keep stdin open. See `docs/STACK_DEMO.md` for the current troubleshooting notes.

## Step 3 — Check runtime status

In the `liminal-cli` session, run:

```text
:status
```

This should show live runtime state. The exact output can evolve while LiminalDB is pre-1.0, but this is the first command to verify the runtime is responsive.

## Step 4 — Plant a Seed Garden goal

Run:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
```

What this means:

- `BoostToken` is the goal/action type for the demo.
- `cpu/load` is the pattern being affected.
- `{"scale":0.2}` is small structured goal metadata.
- `60000` is the time window / TTL-like duration used by the command.

Conceptually:

```text
plant goal -> runtime observes/adapts -> goal state becomes inspectable
```

## Step 5 — Inspect the Seed Garden

Run:

```text
:seed garden
```

This shows the active goal lifecycle state.

Seed Garden is the part of LiminalDB that makes goal-oriented runtime behavior visible. The simple mental model is:

```text
plant -> grow -> harvest
```

It lets the database/runtime represent not just raw events, but directional adaptive work.

## Step 6 — Inspect Mirror Timeline

Run:

```text
:mirror top 5
```

Mirror Timeline is LiminalDB's append-only event inspection surface.

It is useful for:

- replay,
- audit,
- debugging,
- reviewing recent runtime decisions,
- connecting adaptive behavior to concrete events.

The key idea:

```text
runtime action -> event -> timeline -> replayable inspection
```

## Step 7 — Persist a snapshot

Run:

```text
:snapshot
```

This persists current state through the snapshot path.

For a new contributor, this command is useful because it connects the living-system metaphor to concrete database concerns: persistence, recovery, and replay.

## Minimal command sequence

Use this sequence as the fastest demo path:

```text
:status
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
:mirror top 5
:snapshot
```

## How to explain the concepts during the demo

| Concept | Plain-language explanation |
|---|---|
| Cell | A small reactive unit that holds state and responds to patterns. |
| Impulse | A signal sent into the system, such as query/write/affect. |
| TRS | Adaptive control loop that helps the runtime respond to load or stress. |
| Reflex | A feedback rule triggered by runtime conditions. |
| Seed Garden | Goal lifecycle layer: plant, grow, inspect, and later harvest goals. |
| Mirror Timeline | Append-only event trail for replay, audit, and inspection. |

## What this demo proves

This demo shows that a local reviewer/contributor can:

- build the CLI,
- start the local runtime,
- interact with Seed Garden,
- inspect runtime status,
- inspect Mirror Timeline,
- persist a snapshot.

## What this demo does not prove

This demo does not prove:

- production database readiness,
- production security posture,
- multi-node or Raft/consensus readiness,
- superiority over mature databases,
- stable pre-1.0 API or protocol compatibility,
- benchmark performance across workloads.

For benchmark evidence, see:

- `docs/BENCHMARKS.md`
- `docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md`

For the broader stack scenario, see:

- `docs/STACK_DEMO.md`

## Next steps

After this demo, useful next tasks are:

1. verify the quickstart on a clean machine,
2. add WebSocket protocol examples,
3. add TypeScript SDK smoke test,
4. add Seed Garden standalone walkthrough,
5. add benchmark/status badges to README.
