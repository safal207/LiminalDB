#!/usr/bin/env python3
"""Run process-crash and single-writer durability checks for LiminalDB."""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Iterable


SCHEMA = "org.liminaldb.transition-durability-evidence.v0.1"
CRASH_EXIT = 86


def run(
    command: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    expected: Iterable[int] = (0,),
) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    expected_codes = set(expected)
    if completed.returncode not in expected_codes:
        raise RuntimeError(
            "command failed\n"
            f"command: {command}\n"
            f"expected: {sorted(expected_codes)}\n"
            f"actual: {completed.returncode}\n"
            f"stdout:\n{completed.stdout}\n"
            f"stderr:\n{completed.stderr}"
        )
    return completed


def parse_value(output: str, name: str) -> str:
    prefix = f"{name}="
    for line in output.splitlines():
        if line.startswith(prefix):
            return line[len(prefix) :].strip()
    raise RuntimeError(f"missing {name} in output: {output!r}")


def target_binary(workspace: Path) -> Path:
    configured = os.environ.get("CARGO_TARGET_DIR")
    if configured:
        target = Path(configured)
        if not target.is_absolute():
            target = workspace / target
    else:
        target = workspace / "target"
    suffix = ".exe" if os.name == "nt" else ""
    return target / "debug" / "examples" / f"durability_crash_harness{suffix}"


def harness(
    binary: Path,
    *args: str | Path,
    env_extra: dict[str, str] | None = None,
    expected: Iterable[int] = (0,),
) -> subprocess.CompletedProcess[str]:
    environment = os.environ.copy()
    environment.pop("LIMINALDB_FAILPOINT", None)
    environment.pop("LIMINALDB_FAILPOINT_MARKER", None)
    if env_extra:
        environment.update(env_extra)
    return run(
        [str(binary), *(str(value) for value in args)],
        env=environment,
        expected=expected,
    )


def inspect_ledger(binary: Path, root: Path) -> int:
    result = harness(binary, "inspect-ledger", root)
    return int(parse_value(result.stdout, "EVENT_COUNT"))


def inspect_store(binary: Path, root: Path) -> int:
    result = harness(binary, "inspect-store", root)
    return int(parse_value(result.stdout, "RECORD_COUNT"))


def fresh_root(base: Path, name: str) -> Path:
    root = base / name
    if root.exists():
        shutil.rmtree(root)
    return root


def crash_environment(marker: Path, failpoint: str) -> dict[str, str]:
    return {
        "LIMINALDB_FAILPOINT": failpoint,
        "LIMINALDB_FAILPOINT_MARKER": str(marker),
    }


def verify_marker(marker: Path, failpoint: str) -> None:
    if not marker.exists():
        raise RuntimeError(f"failpoint marker was not written: {marker}")
    actual = marker.read_text(encoding="utf-8").strip()
    if actual != failpoint:
        raise RuntimeError(f"failpoint marker mismatch: {actual!r} != {failpoint!r}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parents[1]
    workspace = repo_root / "liminal-db"
    rustc = run(["rustc", "--version"]).stdout.strip()
    cargo = run(["cargo", "--version"]).stdout.strip()

    run(
        [
            "cargo",
            "build",
            "-p",
            "liminal-store",
            "--example",
            "durability_crash_harness",
            "--features",
            "durability-failpoints",
        ],
        cwd=workspace,
    )
    binary = target_binary(workspace)
    if not binary.exists():
        raise RuntimeError(f"durability harness binary not found: {binary}")

    cases: list[dict[str, object]] = []
    verdict = "PASSED"
    failure: str | None = None

    try:
        with tempfile.TemporaryDirectory(prefix="liminaldb-durability-") as directory:
            base = Path(directory)

            acknowledged = fresh_root(base, "acknowledged-append")
            harness(binary, "prepare-ledger", acknowledged)
            harness(binary, "append", acknowledged)
            recovered = inspect_ledger(binary, acknowledged)
            if recovered != 2:
                raise RuntimeError(f"acknowledged append recovered {recovered}, expected 2")
            cases.append(
                {
                    "case_id": "acknowledged_append_survives_restart",
                    "boundary": "append_returned_after_sync_all",
                    "recovered_event_count": recovered,
                    "status": "PASSED",
                }
            )

            append_cases = [
                ("before_wal_write", 1),
                ("after_wal_write_before_flush", 2),
                ("after_wal_flush_before_sync", 2),
                ("after_wal_sync_before_return", 2),
            ]
            for failpoint, expected_count in append_cases:
                root = fresh_root(base, failpoint)
                marker = base / f"{failpoint}.marker"
                harness(binary, "prepare-ledger", root)
                crashed = harness(
                    binary,
                    "crash-append",
                    root,
                    env_extra=crash_environment(marker, failpoint),
                    expected=(CRASH_EXIT,),
                )
                verify_marker(marker, failpoint)
                recovered = inspect_ledger(binary, root)
                if recovered != expected_count:
                    raise RuntimeError(
                        f"{failpoint} recovered {recovered}, expected {expected_count}"
                    )
                cases.append(
                    {
                        "case_id": failpoint,
                        "process_exit": crashed.returncode,
                        "recovered_event_count": recovered,
                        "expected_event_count": expected_count,
                        "status": "PASSED",
                    }
                )

            snapshot_points = [
                "snapshot_during_temp_write",
                "snapshot_after_file_sync_before_rename",
                "snapshot_after_rename_before_directory_sync",
            ]
            for failpoint in snapshot_points:
                root = fresh_root(base, failpoint)
                marker = base / f"{failpoint}.marker"
                harness(binary, "prepare-snapshot", root)
                crashed = harness(
                    binary,
                    "crash-snapshot",
                    root,
                    failpoint,
                    env_extra={"LIMINALDB_FAILPOINT_MARKER": str(marker)},
                    expected=(CRASH_EXIT,),
                )
                verify_marker(marker, failpoint)
                recovered = inspect_ledger(binary, root)
                if recovered != 2:
                    raise RuntimeError(
                        f"{failpoint} recovered {recovered}, expected 2"
                    )
                cases.append(
                    {
                        "case_id": failpoint,
                        "process_exit": crashed.returncode,
                        "recovered_event_count": recovered,
                        "expected_event_count": 2,
                        "status": "PASSED",
                    }
                )

            rotation = fresh_root(base, "wal-segment-rotation")
            rotation_marker = base / "wal-segment-rotation.marker"
            harness(binary, "prepare-rotation", rotation)
            crashed = harness(
                binary,
                "crash-rotation",
                rotation,
                env_extra=crash_environment(
                    rotation_marker, "wal_segment_rotation"
                ),
                expected=(CRASH_EXIT,),
            )
            verify_marker(rotation_marker, "wal_segment_rotation")
            recovered_records = inspect_store(binary, rotation)
            if recovered_records != 1:
                raise RuntimeError(
                    f"rotation crash recovered {recovered_records}, expected 1"
                )
            cases.append(
                {
                    "case_id": "wal_segment_rotation",
                    "process_exit": crashed.returncode,
                    "recovered_record_count": recovered_records,
                    "expected_record_count": 1,
                    "status": "PASSED",
                }
            )

            partial = fresh_root(base, "partial-wal-record")
            harness(binary, "prepare-rotation", partial)
            wal_files = sorted((partial / "data").glob("*.wal"))
            if not wal_files:
                raise RuntimeError("no WAL file found for partial-record test")
            with wal_files[-1].open("ab") as wal:
                wal.write(struct.pack("<I", 16))
                wal.write(b"partial")
                wal.flush()
                os.fsync(wal.fileno())
            rejected = harness(
                binary,
                "inspect-store",
                partial,
                expected=tuple(range(1, 256)),
            )
            cases.append(
                {
                    "case_id": "partial_wal_record_rejected",
                    "process_exit": rejected.returncode,
                    "status": "PASSED",
                }
            )

            lock_root = fresh_root(base, "single-writer-lock")
            ready = base / "writer-ready.marker"
            holder = subprocess.Popen(
                [str(binary), "hold-lock", str(lock_root), str(ready)],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                env=os.environ.copy(),
            )
            try:
                deadline = time.monotonic() + 15
                while not ready.exists() and time.monotonic() < deadline:
                    if holder.poll() is not None:
                        raise RuntimeError("writer-lock holder exited before readiness")
                    time.sleep(0.05)
                if not ready.exists():
                    raise RuntimeError("writer-lock holder did not become ready")

                rejected = harness(
                    binary,
                    "try-open",
                    lock_root,
                    expected=(73,),
                )
                cases.append(
                    {
                        "case_id": "second_process_writer_rejected",
                        "process_exit": rejected.returncode,
                        "status": "PASSED",
                    }
                )
            finally:
                holder.terminate()
                try:
                    holder.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    holder.kill()
                    holder.wait(timeout=10)

            reopened = inspect_store(binary, lock_root)
            if reopened != 0:
                raise RuntimeError(f"lock-release reopen saw {reopened} records")
            cases.append(
                {
                    "case_id": "writer_lock_released_after_process_death",
                    "recovered_record_count": reopened,
                    "status": "PASSED",
                }
            )

    except Exception as error:  # noqa: BLE001 - report evidence before failing CI
        verdict = "FAILED"
        failure = str(error)

    report = {
        "schema": SCHEMA,
        "verdict": verdict,
        "platform": {
            "system": platform.system(),
            "release": platform.release(),
            "machine": platform.machine(),
            "python": sys.version.split()[0],
            "rustc": rustc,
            "cargo": cargo,
        },
        "harness": {
            "feature": "durability-failpoints",
            "crash_exit_code": CRASH_EXIT,
            "case_count": len(cases),
        },
        "cases": cases,
        "claim_boundary": [
            "The matrix simulates abrupt process termination without Rust unwinding or destructors.",
            "It does not simulate sudden power loss, controller-cache loss, or hostile network filesystems.",
            "Directory fsync is performed through the Rust standard library on Unix; Windows has no equivalent standard-library directory fsync primitive.",
            "External anti-rollback protection remains the responsibility of signed checkpoints and external anchors.",
        ],
    }
    if failure is not None:
        report["failure"] = failure

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))

    if verdict != "PASSED":
        raise SystemExit(1)


if __name__ == "__main__":
    main()
