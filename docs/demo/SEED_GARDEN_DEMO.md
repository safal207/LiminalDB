# Seed Garden Demo — Goal Lifecycle in LiminalDB

**Goal:** understand Seed Garden as LiminalDB's goal-oriented runtime layer: plant a goal, inspect its lifecycle, and connect the goal state to runtime status and Mirror Timeline events.

This is a local reviewer/contributor demo. It is not production deployment guidance and does not make production performance claims.

## What Seed Garden means

Seed Garden is the part of LiminalDB that makes adaptive goals visible.

Traditional database operations often look like:

```text
request -> read/write -> response
```

Seed Garden adds a goal lifecycle:

```text
plant -> grow -> inspect -> harvest
```

In plain language:

- **plant** — introduce a goal into the runtime.
- **grow** — let the runtime carry and adapt that goal over time.
- **inspect** — observe goal state through CLI and timeline surfaces.
- **harvest** — resolve or collect the result of goal-oriented work when supported by the runtime path.

## How this fits LiminalDB

Seed Garden sits alongside LiminalDB's other living-system concepts:

```text
impulse -> cell state -> adaptive control / reflexes -> seed goal -> mirror timeline
```

The useful mental model:

| Concept | Role |
|---|---|
| Cell | Reactive unit that holds state and responds to patterns. |
| Impulse | Signal sent into the system. |
| TRS | Adaptive control loop for load/stress response. |
| Reflex | Feedback rule triggered by runtime conditions. |
| Seed Garden | Goal lifecycle layer. |
| Mirror Timeline | Event trail for inspection/replay. |

## Prerequisites

Build `liminal-cli` first:

```bash
cargo build --release -p liminal-cli
```

Start a local runtime:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

On Windows, the repository also provides:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

## Step 1 — Check runtime status

In the `liminal-cli` session:

```text
:status
```

This confirms the runtime is responsive before planting a seed.

## Step 2 — Plant a goal

Run:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
```

What the command expresses:

```text
plant goal type=BoostToken
for pattern=cpu/load
with metadata={"scale":0.2}
for duration/window=60000
```

This is a compact way to say:

> create a small adaptive goal that affects or tracks the `cpu/load` pattern.

## Step 3 — Inspect the garden

Run:

```text
:seed garden
```

This is the main inspection command for Seed Garden state.

A useful reviewer question while reading the output:

```text
Can I see what goals exist, what they target, and whether the runtime is carrying them forward?
```

The exact output may evolve while LiminalDB is pre-1.0, but the purpose should remain stable: make goal lifecycle state visible.

## Step 4 — Inspect runtime status again

Run:

```text
:status
```

This helps connect the goal to the broader runtime state.

The demo is not trying to prove performance. It is showing that a goal can be introduced and then inspected through the local runtime surface.

## Step 5 — Inspect Mirror Timeline

Run:

```text
:mirror top 5
```

Mirror Timeline is the event/replay surface. It helps connect a Seed Garden action to observable runtime history.

Conceptually:

```text
seed command -> runtime event -> mirror timeline -> replayable inspection
```

## Step 6 — Persist state

Run:

```text
:snapshot
```

This links Seed Garden to database concerns: persistence, recovery, and replay.

## Minimal Seed Garden demo script

Use this as the shortest manual path:

```text
:status
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
:status
:mirror top 5
:snapshot
```

## Expected reviewer takeaway

A reviewer should leave with this understanding:

> LiminalDB does not only store passive records. It can represent goal-like adaptive runtime state and expose that state through local inspection and timeline surfaces.

## What this demo proves

This demo shows that a local reviewer/contributor can:

- start the LiminalDB runtime,
- plant a Seed Garden goal,
- inspect the active garden,
- inspect runtime status,
- inspect recent Mirror Timeline events,
- persist a snapshot.

## What this demo does not prove

This demo does not prove:

- production database readiness,
- production security posture,
- stable pre-1.0 Seed Garden API compatibility,
- benchmark performance,
- multi-node behavior,
- full recovery guarantees,
- superiority over mature databases.

For benchmark evidence, see:

- `docs/BENCHMARKS.md`
- `docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md`

For the broader local demo path, see:

- `docs/demo/FIVE_MINUTE_STACK_DEMO.md`
- `docs/STACK_DEMO.md`

## Suggested next improvements

Useful follow-ups:

1. Add captured example output from a clean run.
2. Add a tiny scripted Seed Garden smoke demo.
3. Add a Mirror Timeline example showing the seed-related event path.
4. Add a TypeScript SDK example that observes Seed Garden-related runtime events if supported by the protocol.
5. Add a short architecture note explaining how Seed Garden state is stored and replayed.
