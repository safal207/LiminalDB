#!/usr/bin/env python3
"""Validate the pinned Crashpoint action-readback source against the local mapping fixture."""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


def load(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def fail(message: str) -> None:
    raise SystemExit(f"CRASHPOINT-LIMINALDB-001 VERIFY FAIL: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--fixture", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    parser.add_argument("--prediction", required=True, type=Path)
    args = parser.parse_args()

    fixture = load(args.fixture)
    manifest = load(args.manifest)
    prediction = load(args.prediction)

    if fixture.get("system_case") != "CRASHPOINT-LIMINALDB-001":
        fail("unexpected local system_case")

    source = fixture["source"]
    expected_sha = source["prediction_sha256"]
    actual_sha = hashlib.sha256(args.prediction.read_bytes()).hexdigest()
    if actual_sha != expected_sha:
        fail(f"prediction SHA-256 mismatch: {actual_sha} != {expected_sha}")

    if manifest.get("status") != "COMPLETE":
        fail(f"source manifest status is {manifest.get('status')!r}, not COMPLETE")
    if manifest.get("all_agree") is not True:
        fail("source manifest all_agree is not true")
    if manifest.get("trial_count") != source["trial_count"]:
        fail("source trial_count does not match fixture")
    if manifest.get("expected_trial_count") != source["trial_count"]:
        fail("source expected_trial_count does not match fixture")
    if manifest.get("prediction_sha256") != expected_sha:
        fail("manifest prediction_sha256 does not match pinned prediction")
    if manifest.get("prediction_embedded_sha256") != expected_sha:
        fail("embedded prediction SHA-256 does not match pinned prediction")

    fixture_cases = {case["case_id"]: case for case in fixture["cases"]}
    manifest_cases = manifest.get("cases")
    if manifest_cases != list(fixture_cases):
        fail(f"case order/set drift: {manifest_cases!r}")

    prediction_cases = prediction.get("cases", {})
    if set(prediction_cases) != set(fixture_cases):
        fail("prediction case set does not match fixture case set")

    trials_by_case: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for trial in manifest.get("trials", []):
        trials_by_case[trial["case"]].append(trial)

    if set(trials_by_case) != set(fixture_cases):
        fail("manifest trial case set does not match fixture")

    for case_id, case in fixture_cases.items():
        source_case = case["source"]
        trials = trials_by_case[case_id]
        if len(trials) != source["trials_per_case"]:
            fail(f"{case_id}: expected {source['trials_per_case']} trials, got {len(trials)}")

        expected_fields = {
            "client_claim": source_case["client_claim"],
            "external_outcome": source_case["external_outcome"],
            "observation_availability": source_case["observation_availability"],
            "externally_verified": source_case["externally_verified"],
            "effect_count": source_case["effect_count"],
            "digests_match_admission": source_case["digests_match_admission"],
        }

        for trial in trials:
            if trial.get("passed") is not True:
                fail(f"{trial.get('trial_id')}: source trial is not passing")
            for field, expected in expected_fields.items():
                if trial.get(field) != expected:
                    fail(
                        f"{trial.get('trial_id')}: {field}={trial.get(field)!r}, "
                        f"expected {expected!r}"
                    )

        predicted = prediction_cases[case_id]
        for field in (
            "client_claim",
            "external_outcome",
            "observation_availability",
            "externally_verified",
        ):
            if predicted.get(field) != source_case[field]:
                fail(f"{case_id}: prediction {field} drift")

    effect_counts = Counter(
        trial.get("effect_count") for trial in manifest["trials"]
    )

    report = {
        "system_case": "CRASHPOINT-LIMINALDB-001",
        "status": "VERIFIED_SOURCE_MAPPING",
        "source_publication_commit": source["publication_commit"],
        "prediction_sha256": actual_sha,
        "cases": len(fixture_cases),
        "trials": manifest["trial_count"],
        "effect_count_distribution": {
            "null": effect_counts.get(None, 0),
            "0": effect_counts.get(0, 0),
            "1": effect_counts.get(1, 0),
            "2": effect_counts.get(2, 0),
        },
        "boundary": (
            "Pinned source facts verified against manifest/prediction; "
            "this does not rerun the Crashpoint producer/verifier or upgrade its scope."
        ),
    }
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
