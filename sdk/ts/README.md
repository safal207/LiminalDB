# LiminalDB TypeScript SDK

This package contains the TypeScript client wrapper for the LiminalDB WebSocket protocol.

The SDK uses generated command/event types from:

```text
src/protocol-types.ts
```

The main client is:

```text
src/client.ts
```

## Build

From `sdk/ts`:

```bash
npm install
npm run build
```

## Local smoke example

Start LiminalDB from the repository root first:

```bash
cargo build --release -p liminal-cli
./target/release/liminal-cli --store ./data --ws-port 8787
```

Then, from `sdk/ts`:

```bash
npm install
npm run smoke
```

The smoke example connects to:

```text
ws://127.0.0.1:8787
```

It sends a small set of SDK commands:

- `subscribe('cpu/load')`
- `lql('SELECT cpu/load WINDOW 1000')`
- `seed.garden()`
- `mirror.timeline({ from: 'now-5m', to: 'now' })`
- `unsubscribe(subscriptionId)`

By default, the smoke script treats connect + send as sufficient because not every local runtime state produces immediate events during the short wait window.

To require at least one observed event:

```bash
npm run smoke:require-event
```

## Environment variables

| Variable | Default | Meaning |
|---|---:|---|
| `LIMINAL_WS_URL` | `ws://127.0.0.1:8787` | WebSocket endpoint. |
| `LIMINAL_SMOKE_TIMEOUT_MS` | `5000` | Connection timeout. |
| `LIMINAL_SMOKE_EVENT_WAIT_MS` | `2000` | How long to wait for events after sending commands. |
| `LIMINAL_SMOKE_REQUIRE_EVENT` | unset | Set to `1` to fail if no event is observed. |

Example:

```bash
LIMINAL_WS_URL=ws://127.0.0.1:8787 \
LIMINAL_SMOKE_EVENT_WAIT_MS=5000 \
npm run smoke
```

## Raw protocol vs SDK envelope

The raw WebSocket examples use the protocol-level shape:

```json
{"cmd":"lql","q":"SELECT cpu/load WINDOW 1000"}
```

The SDK sends generated command envelopes:

```json
{"version":"1.0.0","command":{"op":"lql","id":"...","query":"SELECT cpu/load WINDOW 1000"}}
```

For raw protocol examples, see:

```text
../../docs/api/WEBSOCKET_EXAMPLES.md
```

For protocol compatibility notes, see:

```text
../../docs/RELEASE_COMPATIBILITY.md
```

## Scope

This smoke example is a local development check. It does not claim stable pre-1.0 protocol compatibility, production reliability, or complete SDK coverage.
