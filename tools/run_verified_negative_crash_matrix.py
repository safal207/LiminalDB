#!/usr/bin/env python3
"""Run exact verified-negative snapshot crash recovery across supported platforms."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

FAILPOINTS = (
    "snapshot_during_temp_write",
    "snapshot_after_file_sync_before_rename",
    "snapshot_after_rename_before_directory_sync",
)
RECEIPT_PREFIX = "VERIFIED_NEGATIVE_CRASH_RECEIPT="
EXPECTED_TRANSITION_ID = "airbnb-garden-29702510829"
EXPECTED_CONTINUITY_REF = (
    "sha256:200e0076823c241cdb05db79fce50ad51bef01f4f0456e8fa2ebd93ae2809619"
)


def run(
    command: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )


def require_success(result: subprocess.CompletedProcess[str], context: str) -> None:
    if result.returncode != 0:
        raise RuntimeError(f"{context} failed with {result.returncode}:\n{result.stdout}")


def parse_receipt(output: str) -> dict[str, object]:
    lines = [line for line in output.splitlines() if line.startswith(RECEIPT_PREFIX)]
    if len(lines) != 1:
        raise RuntimeError(f"expected one crash receipt, found {len(lines)}:\n{output}")
    receipt = json.loads(lines[0][len(RECEIPT_PREFIX) :])
    if receipt.get("transition_id") != EXPECTED_TRANSITION_ID:
        raise RuntimeError(
            f"transition identity mismatch: {receipt.get('transition_id')!r}"
        )
    if receipt.get("continuity_ref") != EXPECTED_CONTINUITY_REF:
        raise RuntimeError(
            f"continuity identity mismatch: {receipt.get('continuity_ref')!r}"
        )
    if receipt.get("event_count") != 6:
        raise RuntimeError("recovered event count is not six")
    if receipt.get("recovered") is not True:
        raise RuntimeError("recovery was not confirmed")
    if receipt.get("recovery_artifacts_removed") is not True:
        raise RuntimeError("temporary snapshot state was not removed")
    if receipt.get("partial_projection_observed") is not False:
        raise RuntimeError("partial projection was observed")
    dimensions = receipt.get("dimensions")
    if dimensions != {
        "authority": "VALID",
        "execution": "OBSERVED_EXECUTED",
        "response_integrity": "VERIFIED",
        "causal_validity": "NOT_EVALUATED",
        "continuity_posture": "REPORT_ONLY",
    }:
        raise RuntimeError(f"dimension boundary mismatch: {dimensions!r}")
    memory = receipt.get("memory")
    if memory != {"durable_memory_accepted": False, "production_write": False}:
        raise RuntimeError(f"memory boundary mismatch: {memory!r}")
    authority = receipt.get("authority")
    if authority != {
        "external_submission": False,
        "deployment": False,
        "merge": False,
    }:
        raise RuntimeError(f"authority boundary mismatch: {authority!r}")
    return receipt


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    repository_root = Path(__file__).resolve().parents[1]
    workspace = repository_root / "liminal-db"
    executable_suffix = ".exe" if os.name == "nt" else ""

    build = run(
        [
            "cargo",
            "build",
            "-p",
            "liminal-store",
            "--features",
            "durability-failpoints",
            "--example",
            "verified_negative_crash_harness",
            "--example",
            "verified_negative_recover_inspect",
        ],
        cwd=workspace,
    )
    require_success(build, "build verified-negative crash harnesses")

    binary_root = workspace / "target" / "debug" / "examples"
    harness = binary_root / f"verified_negative_crash_harness{executable_suffix}"
    inspector = binary_root / f"verified_negative_recover_inspect{executable_suffix}"
    if not harness.is_file() or not inspector.is_file():
        raise RuntimeError("expected crash harness executables were not built")

    cases: list[dict[str, object]] = []
    for failpoint in FAILPOINTS:
        with tempfile.TemporaryDirectory(
            prefix=f"liminaldb-negative-{failpoint}-"
        ) as temporary:
            root = Path(temporary) / "ledger"
            marker = Path(temporary) / "failpoint.marker"

            prepared = run([str(harness), "prepare", str(root)], cwd=workspace)
            require_success(prepared, f"prepare {failpoint}")

            environment = os.environ.copy()
            environment["LIMINALDB_FAILPOINT"] = failpoint
            environment["LIMINALDB_FAILPOINT_MARKER"] = str(marker)
            crashed = run(
                [str(harness), "crash-snapshot", str(root)],
                cwd=workspace,
                env=environment,
            )
            if crashed.returncode != 86:
                raise RuntimeError(
                    f"{failpoint} returned {crashed.returncode}, expected 86:\n"
                    f"{crashed.stdout}"
                )
            if (
                not marker.is_file()
                or marker.read_text(encoding="utf-8").strip() != failpoint
            ):
                raise RuntimeError(f"{failpoint} marker was not durably recorded")

            inspected = run([str(inspector), str(root)], cwd=workspace)
            require_success(inspected, f"recover and inspect {failpoint}")
            receipt = parse_receipt(inspected.stdout)
            receipt["failpoint"] = failpoint
            receipt["crash_exit_code"] = crashed.returncode
            cases.append(receipt)

    summary = {
        "schema_version": "liminaldb-verified-negative-crash-matrix-v0.1",
        "platform": sys.platform,
        "case_count": len(cases),
        "all_recovered": all(case["recovered"] is True for case in cases),
        "partial_projection_count": sum(
            1 for case in cases if case["partial_projection_observed"] is True
        ),
        "production_write": False,
        "durable_memory_accepted": False,
        "sudden_power_loss_claimed": False,
        "cases": cases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
