# LiminalDB Quickstart Validation Snapshot

**Status:** reviewer-facing validation report
**Source of truth:** [`README.md`](../../README.md), [`scripts/demo-stack.ps1`](../../scripts/demo-stack.ps1)
**Scope:** single-machine, Windows 11 + Rust 1.89 MSVC, validation of the README quickstart and the optional Windows demo entrypoint.
**Tracks:** [`safal207/LiminalBD#57`](https://github.com/safal207/LiminalBD/issues/57) (good first issue: verify LiminalDB quickstart on a clean machine).

This document records the result of running the documented quickstart commands on one Windows machine, with all artifacts pinned to a specific repository commit. It is an evidence note, not a benchmark.

## Executive summary

| Step | Command | Status |
|---|---|---|
| Build | `cargo build --release -p liminal-cli` | PASS |
| Tests | `cargo test --workspace` | PASS (72 unit tests, 0 failed) |
| CLI smoke | `./target/release/liminal-cli --store ./data --ws-port 8787` | PASS (`ws_server.listening`, TCP connect succeeds) |
| Demo stack | `powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1` | PASS (both MSVC binaries produced) |

All four documented quickstart paths run end-to-end on the validated machine. README requires no breaking changes; one small Windows clarification is added in the same PR.

## Environment

- Date: `2026-05-10`
- Repository commit: `f94f4d139baccc0588203de8780fc7bf363990e0` (branch `main` at the moment of validation)
- OS: Windows 11 Pro, version `10.0.26200` (build `26200`), 64-bit
- CPU architecture: `x86_64`
- Rust toolchain: `rustc 1.89.0 (29483883e 2025-08-04)`, host `x86_64-pc-windows-msvc`
- Cargo: `cargo 1.89.0 (c24e10642 2025-06-23)`
- Active default toolchain: `stable-x86_64-pc-windows-msvc`
- Shell: PowerShell 5.1 (Windows PowerShell)

The MSVC toolchain is installed and active by default on this machine, so the `scripts/demo-stack.ps1` `cargo +stable-x86_64-pc-windows-msvc` invocation does not require any extra setup. On machines where MSVC is not the default, follow [`docs/WINDOWS_RUST.md`](../WINDOWS_RUST.md) before running the demo script.

## Build

Command:

```powershell
cargo build --release -p liminal-cli
```

Result:

- Exit code: `0`
- Wall time: `49.77 s` (first cold build of the release profile)
- Output: `target\release\liminal-cli.exe` produced.
- No warnings emitted from `liminal-cli` itself in this profile.

## Tests

Command:

```powershell
cargo test --workspace
```

Result:

- Exit code: `0`
- Wall time: `32.17 s` (test profile build + run)
- Unit tests: `72 passed`, `0 failed`, `0 ignored`.
- Doc tests: `0` across all crates.

Per-crate breakdown:

| Crate | Passed | Failed | Notes |
|---|---|---|---|
| `liminal-bridge-abi` | 6 | 0 | protocol round-trips + FFI flow |
| `liminal-bridge-net` | 10 | 0 | CRDT, consensus, peers, stream codec |
| `liminal-cli` | 5 | 0 | event formatters and harmony snapshot |
| `liminal-core` | 46 | 0 | cluster_field, lql, awakening, mirror, security, symmetry, trs, variant, views |
| `liminal-sensor` | 0 | 0 | no unit tests |
| `liminal-store` | 5 | 0 | snapshot, wal round-trip, journal replay |
| `liminaldb-client` | 0 | 0 | library; smoke is via examples |
| `liminaldb-conformance` | 0 | 0 | binary harness |
| `liminaldb-protocol-codegen` | 0 | 0 | codegen utility |

Non-blocking warnings observed during the test build (no-op for issue #57, listed for completeness):

- `conformance/src/main.rs:25` — `dead_code`: variants `Passed` and `Failed` of `ScenarioStatus` are never constructed.
- `sdk/rust/examples/seeds_demo.rs:18` — `unused_must_use`: `client.seed_abort(...)` returns a `Future` that is dropped without `.await`.

These are pre-existing and unrelated to the quickstart paths.

## CLI smoke

Command (as written in [`README.md`](../../README.md) Quick Start, started directly):

```powershell
.\target\release\liminal-cli.exe --store .\smoke-data --ws-port 8787
```

Result:

- Process stays alive after launch.
- Both expected log markers appear immediately:
  - `INFO liminal_cli: ws.local_listening addr=127.0.0.1:8787`
  - `INFO liminal_bridge_net::ws_server: ws_server.listening addr=127.0.0.1:8787`
- TCP connect to `127.0.0.1:8787` from a second PowerShell session succeeds (`System.Net.Sockets.TcpClient.Connect` returns without error).
- A raw (non-WebSocket) TCP probe is correctly rejected by the WS server with `ws_server.accept_failed ... error=WebSocket protocol error: Handshake not finished`, which is expected behaviour.
- After ~5 seconds the runtime starts emitting periodic `IMPULSE`, `HARMONY`, `METRICS`, and `trs_trace` events, confirming the adaptive control loop is active.

The README's `Expected output` section is therefore accurate: both `ws.local_listening` and `ws_server.listening addr=127.0.0.1:8787` markers do appear in CLI logs.

### Note on the README troubleshooting hint

The README contains a Windows hint:

> If `liminal-cli` exits immediately on Windows, keep stdin open (for example with `cmd /c "ping -t 127.0.0.1 | ..."`).

On this validated environment the workaround was **not** needed — direct invocation kept the process alive. When that workaround was applied anyway (`cmd /c "ping -t 127.0.0.1 | .\target\release\liminal-cli.exe --store .\smoke-data --ws-port 8787"`), `liminal-cli` exited within ~5 seconds even though the wrapper `cmd.exe` stayed alive. The two `listening` log lines were emitted before exit, but TCP connect to `127.0.0.1:8787` failed afterwards.

This is documented as an observation only — the hint may still be required on other Windows configurations, so we are not removing it. README is updated to mention that direct invocation is the preferred path on Windows 11 with the MSVC toolchain, and that the `ping -t` hint is a fallback.

## Demo stack (optional)

Command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

Result:

- Exit code: `0`
- Wall time on a clean MSVC target: ~`50 s` build (cold) + script overhead. Wall time with a warm cache: `5.49 s`.
- Binaries produced:
  - `target\x86_64-pc-windows-msvc\release\liminal-cli.exe` (size: `8 027 648` bytes)
  - `target\x86_64-pc-windows-msvc\release\examples\live-benchmark.exe`
- Script then prints, but does not run, the server and benchmark commands and the expected output markers, exactly as designed.

The script `[1/3]` build phase relies on the MSVC toolchain being installed. On the validated machine MSVC was already the active default, so no extra `rustup toolchain install stable-x86_64-pc-windows-msvc` step was required. Reviewers without MSVC should follow [`docs/WINDOWS_RUST.md`](../WINDOWS_RUST.md) first.

### Cache caveat encountered

When the demo script was run **after** a prior `cargo build --release -p liminal-cli` (default target, no `--target` flag), the first invocation of `scripts/demo-stack.ps1` reported `Finished` in ~14 s but did not produce executables under `target\x86_64-pc-windows-msvc\release\`. After `cargo clean --target x86_64-pc-windows-msvc` and a re-run, both binaries appeared in the expected paths. This is a Cargo cache-state quirk that does not affect a truly clean checkout (which is the scenario issue #57 asks about) and is not a quickstart blocker.

## Observations

- The README quickstart (`cargo build --release -p liminal-cli` followed by `./target/release/liminal-cli --store ./data --ws-port 8787`) works as written on this Windows 11 + Rust 1.89 environment.
- Workspace-wide `cargo test --workspace` passes cleanly with no test failures and no doc-test failures.
- Two non-blocking compiler warnings are pre-existing and outside the scope of issue #57.
- The optional demo stack entrypoint produces both expected binaries when MSVC is the active toolchain.
- The Windows-specific `ping -t` stdin keepalive hint in the README appears to be a fallback rather than the recommended path on a current Windows 11 + MSVC setup.

## Reproducibility checklist

- [x] OS and Rust version recorded above.
- [x] `cargo build --release -p liminal-cli` succeeds.
- [x] `cargo test --workspace` succeeds.
- [x] Demo stack tested end-to-end (build phase).
- [x] README is patched in the same PR for the one Windows-specific clarification.
- [x] Platform-specific issues documented in the [`Note on the README troubleshooting hint`](#note-on-the-readme-troubleshooting-hint) and [`Cache caveat encountered`](#cache-caveat-encountered) sections.

## What this evidence does not prove

- It does not prove cross-OS quickstart parity (Linux and macOS are not validated here; tracked separately).
- It does not prove long-running stability beyond the 5–10 second smoke window.
- It does not prove production-grade behaviour — see [`docs/evidence/BENCHMARK_EVIDENCE_SNAPSHOT.md`](BENCHMARK_EVIDENCE_SNAPSHOT.md) for benchmark scope and caveats.
- It does not validate the GardenLiminal or DAO_lim portions of [`docs/STACK_DEMO.md`](../STACK_DEMO.md); only the LiminalBD-side build was exercised.

## Reviewer interpretation

The right reviewer interpretation is:

> On one Windows 11 machine with Rust 1.89 MSVC and commit `f94f4d1`, the documented LiminalDB quickstart (build, tests, CLI smoke, optional demo script) runs to completion without modifications.

The wrong reviewer interpretation is:

> The LiminalDB quickstart is universally validated across OS, Rust versions, and toolchains.
