# WebSocket Examples — LiminalDB

**Goal:** provide copy-pasteable local examples for interacting with a running LiminalDB WebSocket endpoint.

This document is for contributors and reviewers. It is not a stable API guarantee while LiminalDB is pre-1.0.

Canonical protocol reference:

```text
liminal-db/docs/PROTOCOL.md
```

TypeScript SDK source:

```text
sdk/ts/src/client.ts
sdk/ts/src/protocol-types.ts
```

## Start a local WebSocket runtime

Build the CLI:

```bash
cargo build --release -p liminal-cli
```

Start LiminalDB with a local WebSocket endpoint:

```bash
./target/release/liminal-cli --store ./data --ws-port 8787
```

Expected endpoint:

```text
ws://127.0.0.1:8787
```

Useful CLI checks in the running session:

```text
:status
:mirror top 5
```

On Windows, the repository also includes:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

## Raw WebSocket JSON examples

The protocol documentation describes the Nexus Bridge WebSocket path as accepting JSON text or CBOR binary messages.

For quick local experiments, JSON is easiest.

### Send an impulse

```json
{"cmd":"impulse","data":{"pattern":"cpu/load","kind":"query","strength":0.8}}
```

Conceptually:

```text
client -> impulse -> LiminalDB core -> runtime events / metrics
```

### Run an LQL query

```json
{"cmd":"lql","q":"SELECT cpu/load WINDOW 1000"}
```

Expected event family:

```json
{"ev":"lql","meta":{"select":{}}}
```

Exact response fields can evolve while the protocol is pre-1.0.

### Subscribe to a pattern

```json
{"cmd":"subscribe","pattern":"mem/free"}
```

This is described as syntactic sugar for an LQL subscription path.

Expected event family:

```json
{"ev":"view","meta":{"id":1,"pattern":"mem/free","stats":{}}}
```

### Inspect Mirror Timeline

```json
{"cmd":"mirror.timeline","top":20}
```

This asks for recent timeline entries. It is useful after running CLI commands such as:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:mirror top 5
```

### Trigger dream cycle

```json
{"cmd":"dream.now"}
```

Expected event family:

```json
{"ev":"dream","meta":{"strengthened":0,"weakened":0,"pruned":0,"rewired":0,"protected":0,"took_ms":0}}
```

The concrete numbers depend on runtime state.

## Try raw JSON with websocat

If `websocat` is installed:

```bash
echo '{"cmd":"lql","q":"SELECT cpu/load WINDOW 1000"}' | websocat -n1 ws://127.0.0.1:8787
```

Mirror Timeline example:

```bash
echo '{"cmd":"mirror.timeline","top":20}' | websocat -n1 ws://127.0.0.1:8787
```

Impulse example:

```bash
echo '{"cmd":"impulse","data":{"pattern":"cpu/load","kind":"query","strength":0.8}}' | websocat -n1 ws://127.0.0.1:8787
```

## TypeScript SDK examples

The TypeScript SDK wraps commands in a generated command envelope.

Current SDK send shape from `sdk/ts/src/client.ts`:

```json
{"version":"1.0.0","command":{"op":"lql","id":"...","query":"SELECT cpu/load WINDOW 1000"}}
```

This is different from the raw `cmd` examples above. Use raw JSON examples when testing the documented wire protocol directly. Use the SDK examples when testing `sdk/ts` behavior.

### Minimal browser-like example

```typescript
import { Client } from './sdk/ts/src/client';

const client = new Client({
  url: 'ws://127.0.0.1:8787',
  reconnect: true,
  telemetry: (metrics) => console.log('metrics', metrics),
});

client.on('lql', (event) => {
  console.log('lql event', event);
});

client.on('view', (event) => {
  console.log('view event', event);
});

client.on('seed', (event) => {
  console.log('seed event', event);
});

client.connect();

client.lql('SELECT cpu/load WINDOW 1000');
client.subscribe('cpu/load', { mode: 'live', limit: 10 });
client.seed.garden();
```

### SDK auth example

If auth is enabled in the runtime profile, pass credentials into the SDK:

```typescript
import { Client } from './sdk/ts/src/client';

const client = new Client({
  url: 'ws://127.0.0.1:8787',
  keyId: 'k-alpha',
  secret: 'plaintext',
  ns: 'alpha',
});

client.connect();
```

The raw protocol auth handshake is documented as:

```json
{"cmd":"auth","key_id":"k-alpha","secret":"plaintext","ns":"alpha"}
```

Successful raw auth event shape:

```json
{"ev":"auth","ok":true,"role":"Writer","ns":"alpha"}
```

## Common event families

The raw protocol docs mention these event families:

| Raw event | Meaning |
|---|---|
| `lql` | LQL query or subscription response. |
| `view` | Live view/subscription update. |
| `harmony` | Harmony/TRS or symmetry loop event depending on fields. |
| `dream` | Dream cycle report. |
| `collective_dream` | Synchrony / collective dream report. |
| `snapshot` | Snapshot event. |
| `audit` | Audit or authorization-related event. |
| `alert` | Alert, quota, or runtime warning event. |

The TypeScript SDK normalizes events around generated `kind` values such as:

```text
lql, view, harmony, dream, collective_dream, awaken, echo, status, explain, seed, mirror, noetic, audit, alert
```

## LQL examples

Raw JSON:

```json
{"cmd":"lql","q":"SELECT cpu/load WHERE strength>=0.7 WINDOW 1000"}
```

CLI equivalent:

```text
lql SELECT cpu/load WHERE strength>=0.7 WINDOW 1000
```

Subscription:

```json
{"cmd":"lql","q":"SUBSCRIBE * WHERE adreno=true WINDOW 60000 EVERY 5000"}
```

Unsubscribe:

```json
{"cmd":"lql","q":"UNSUBSCRIBE 1"}
```

## Seed Garden over SDK

The TypeScript SDK exposes a Seed Garden helper:

```typescript
client.seed.plant({
  kind: 'BoostToken',
  target: 'cpu/load',
  args: { scale: 0.2 },
  ttlMs: 60000,
});

client.seed.garden();
```

CLI equivalent:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:seed garden
```

Because the project is pre-1.0, confirm the exact accepted seed payload shape against the current runtime before depending on it externally.

## Mirror Timeline over SDK

The TypeScript SDK exposes a Mirror helper:

```typescript
client.mirror.timeline({
  from: 'now-5m',
  to: 'now',
});
```

Raw protocol example:

```json
{"cmd":"mirror.timeline","top":20}
```

The raw and SDK examples are intentionally shown separately because their envelope shapes differ.

## Troubleshooting

### Connection refused

Check that `liminal-cli` is still running and listening on the expected port:

```text
ws://127.0.0.1:8787
```

If the CLI exits immediately on Windows, see:

```text
docs/STACK_DEMO.md
```

### No events received

Try a simple LQL or Mirror Timeline query first:

```bash
echo '{"cmd":"lql","q":"SELECT cpu/load WINDOW 1000"}' | websocat -n1 ws://127.0.0.1:8787
```

Then generate visible activity through CLI:

```text
:seed plant BoostToken cpu/load {"scale":0.2} 60000
:mirror top 5
```

### SDK does not match raw protocol examples

This can happen because raw protocol examples use `cmd`, while the TypeScript SDK sends generated command envelopes with `op` inside `{ version, command }`.

When debugging:

1. Use raw JSON examples to check the server protocol path.
2. Use SDK examples to check client wrapper behavior.
3. Compare against `sdk/ts/src/protocol-types.ts` and `liminal-db/docs/PROTOCOL.md`.

## Boundaries

These examples do not claim:

- stable pre-1.0 protocol compatibility,
- production authentication posture,
- production WebSocket scaling guarantees,
- complete SDK coverage of every raw protocol command,
- production performance or reliability.

For measured benchmark evidence, see:

- `docs/BENCHMARKS.md`
- `docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md`

For local runtime walkthroughs, see:

- `docs/demo/FIVE_MINUTE_STACK_DEMO.md`
- `docs/demo/SEED_GARDEN_DEMO.md`
