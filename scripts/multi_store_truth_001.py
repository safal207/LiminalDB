#!/usr/bin/env python3
"""Prepare and verify MULTI-STORE-TRUTH-001.

This is a bounded counterfactual knowledge-replay experiment over the pinned
Crashpoint action/readback publication and the existing LiminalDB mapping.
It does not claim that the XTDB transaction times are the original source
event times.
"""

from __future__ import annotations

import argparse
import csv
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

from xtdb_liminaldb_001 import (
    load_json,
    mapped_case_index,
    sql_bool,
    sql_int,
    sql_text,
    sql_timestamp,
    validate_inputs,
)


SYSTEM_CASE = "MULTI-STORE-TRUTH-001"
XTDB_VERSION = "2.1.0"
TABLE = "multi_store_truth_evidence"


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


COLUMNS = [
    "_id",
    "trial_id",
    "case_id",
    "knowledge_state",
    "readback_stage",
    "source_observation_availability",
    "client_claim",
    "external_outcome",
    "observation_availability",
    "externally_verified",
    "effect_count",
    "digests_match_admission",
    "liminal_execution",
    "liminal_response_integrity",
    "liminal_causal_validity",
    "liminal_continuity_posture",
    "side_effect_committed",
    "source_publication_commit",
    "_valid_from",
]


def row_values(
    trial: dict[str, Any],
    mapped_case: dict[str, Any],
    phase: str,
    publication_commit: str,
) -> list[str]:
    liminal = mapped_case["liminal"]

    if phase == "claim":
        knowledge_state = "CLAIM_ONLY"
        readback_stage = "NOT_ATTEMPTED"
        external_outcome = "INDETERMINATE"
        observation_availability = "NOT_READ_BACK"
        externally_verified = False
        effect_count = None
        digests_match = None
        execution = "NOT_OBSERVED"
        response_integrity = "UNKNOWN"
        causal_validity = "NOT_EVALUATED"
        continuity_posture = "REVALIDATE"
        side_effect_committed = None
    elif phase == "unavailable":
        knowledge_state = "READBACK_UNAVAILABLE"
        readback_stage = (
            "SOURCE_UNAVAILABLE"
            if trial["observation_availability"] == "UNAVAILABLE"
            else "INJECTED_VISIBILITY_GAP"
        )
        external_outcome = "INDETERMINATE"
        observation_availability = "UNAVAILABLE"
        externally_verified = False
        effect_count = None
        digests_match = None
        execution = "NOT_OBSERVED"
        response_integrity = "UNKNOWN"
        causal_validity = "NOT_EVALUATED"
        continuity_posture = "REVALIDATE"
        side_effect_committed = None
    elif phase == "readback":
        if trial["observation_availability"] != "FULL":
            raise AssertionError("authoritative readback phase requires FULL source readback")
        knowledge_state = "AUTHORITATIVE_READBACK"
        readback_stage = "AUTHORITATIVE_SOURCE_READBACK"
        external_outcome = trial["external_outcome"]
        observation_availability = trial["observation_availability"]
        externally_verified = trial["externally_verified"]
        effect_count = trial["effect_count"]
        digests_match = trial["digests_match_admission"]
        execution = liminal["execution"]
        response_integrity = liminal["response_integrity"]
        causal_validity = liminal["causal_validity"]
        continuity_posture = liminal["continuity_posture"]
        side_effect_committed = liminal["side_effect_committed"]
    else:
        raise AssertionError(phase)

    return [
        sql_text(trial["action_id"]),
        sql_text(trial["trial_id"]),
        sql_text(trial["case"]),
        sql_text(knowledge_state),
        sql_text(readback_stage),
        sql_text(trial["observation_availability"]),
        sql_text(trial["client_claim"]),
        sql_text(external_outcome),
        sql_text(observation_availability),
        sql_bool(externally_verified),
        sql_int(effect_count),
        sql_bool(digests_match),
        sql_text(execution),
        sql_text(response_integrity),
        sql_text(causal_validity),
        sql_text(continuity_posture),
        sql_bool(side_effect_committed),
        sql_text(publication_commit),
        sql_timestamp(trial["admitted_at_utc"]),
    ]


def emit_insert(
    trials: list[dict[str, Any]],
    cases: dict[str, dict[str, Any]],
    phase: str,
    publication_commit: str,
) -> str:
    rows = [
        "(" + ", ".join(row_values(trial, cases[trial["case"]], phase, publication_commit)) + ")"
        for trial in trials
    ]
    if not rows:
        fail(f"phase {phase!r} has no rows")
    return (
        f"INSERT INTO {TABLE} (" + ", ".join(COLUMNS) + ") VALUES\n  "
        + ",\n  ".join(rows)
        + ";\n"
    )


def prepare(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    manifest = load_json(args.manifest)
    validate_inputs(fixture, manifest)

    trials = sorted(manifest["trials"], key=lambda trial: trial["trial_id"])
    cases = mapped_case_index(fixture)
    publication_commit = fixture["source"]["publication_commit"]
    resolvable = [trial for trial in trials if trial["observation_availability"] == "FULL"]
    unresolved = [trial for trial in trials if trial["observation_availability"] == "UNAVAILABLE"]

    if len(resolvable) != 15 or len(unresolved) != 3:
        fail(
            "expected 15 FULL and 3 UNAVAILABLE source trials; "
            f"got {len(resolvable)} FULL and {len(unresolved)} UNAVAILABLE"
        )

    args.phase_a_sql.write_text(
        emit_insert(trials, cases, "claim", publication_commit),
        encoding="utf-8",
    )
    args.phase_b_sql.write_text(
        emit_insert(trials, cases, "unavailable", publication_commit),
        encoding="utf-8",
    )
    args.phase_c_sql.write_text(
        emit_insert(resolvable, cases, "readback", publication_commit),
        encoding="utf-8",
    )

    report = {
        "system_case": SYSTEM_CASE,
        "xtdb_version": XTDB_VERSION,
        "status": "PREPARED",
        "phase_a_rows": len(trials),
        "phase_b_rows": len(trials),
        "phase_c_rows": len(resolvable),
        "terminal_unavailable_rows": len(unresolved),
        "total_expected_system_versions": len(trials) * 2 + len(resolvable),
        "source_publication_commit": publication_commit,
        "boundary": (
            "Phase B is an explicit projection-level readback visibility fault for source-FULL "
            "trials, while source-UNAVAILABLE trials remain genuinely unavailable. No phase "
            "upgrades missing readback into NO_EFFECT or replay permission."
        ),
    }
    print(json.dumps(report, indent=2, sort_keys=True))


def parse_bool(value: str) -> bool | None:
    value = value.strip().lower()
    if value == "":
        return None
    if value in {"t", "true"}:
        return True
    if value in {"f", "false"}:
        return False
    fail(f"unexpected boolean value in CSV: {value!r}")


def parse_int(value: str) -> int | None:
    value = value.strip()
    return None if value == "" else int(value)


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def assert_common_source(row: dict[str, str], trial: dict[str, Any]) -> None:
    if row["_id"] != trial["action_id"]:
        fail(f"{trial['trial_id']}: action identity drift")
    if row["trial_id"] != trial["trial_id"]:
        fail(f"{trial['trial_id']}: trial identity drift")
    if row["case_id"] != trial["case"]:
        fail(f"{trial['trial_id']}: case identity drift")
    if row["source_observation_availability"] != trial["observation_availability"]:
        fail(f"{trial['trial_id']}: source availability provenance drift")
    if row["client_claim"] != trial["client_claim"]:
        fail(f"{trial['trial_id']}: client claim drift")


def assert_claim(row: dict[str, str], trial: dict[str, Any]) -> None:
    assert_common_source(row, trial)
    expected = {
        "knowledge_state": "CLAIM_ONLY",
        "readback_stage": "NOT_ATTEMPTED",
        "external_outcome": "INDETERMINATE",
        "observation_availability": "NOT_READ_BACK",
        "liminal_execution": "NOT_OBSERVED",
        "liminal_response_integrity": "UNKNOWN",
        "liminal_causal_validity": "NOT_EVALUATED",
        "liminal_continuity_posture": "REVALIDATE",
    }
    for field, value in expected.items():
        if row[field] != value:
            fail(f"{trial['trial_id']}: phase-A {field} drift")
    if parse_bool(row["externally_verified"]) is not False:
        fail(f"{trial['trial_id']}: phase-A fabricated verification")
    if parse_int(row["effect_count"]) is not None:
        fail(f"{trial['trial_id']}: phase-A fabricated effect count")
    if parse_bool(row["digests_match_admission"]) is not None:
        fail(f"{trial['trial_id']}: phase-A fabricated digest comparison")
    if parse_bool(row["side_effect_committed"]) is not None:
        fail(f"{trial['trial_id']}: phase-A fabricated committed-effect knowledge")


def assert_unavailable(row: dict[str, str], trial: dict[str, Any]) -> None:
    assert_common_source(row, trial)
    expected_stage = (
        "SOURCE_UNAVAILABLE"
        if trial["observation_availability"] == "UNAVAILABLE"
        else "INJECTED_VISIBILITY_GAP"
    )
    expected = {
        "knowledge_state": "READBACK_UNAVAILABLE",
        "readback_stage": expected_stage,
        "external_outcome": "INDETERMINATE",
        "observation_availability": "UNAVAILABLE",
        "liminal_execution": "NOT_OBSERVED",
        "liminal_response_integrity": "UNKNOWN",
        "liminal_causal_validity": "NOT_EVALUATED",
        "liminal_continuity_posture": "REVALIDATE",
    }
    for field, value in expected.items():
        if row[field] != value:
            fail(f"{trial['trial_id']}: phase-B {field}={row[field]!r}, expected {value!r}")
    if parse_bool(row["externally_verified"]) is not False:
        fail(f"{trial['trial_id']}: unavailable readback marked externally verified")
    if parse_int(row["effect_count"]) is not None:
        fail(f"{trial['trial_id']}: unavailable readback fabricated effect count")
    if parse_bool(row["digests_match_admission"]) is not None:
        fail(f"{trial['trial_id']}: unavailable readback fabricated digest comparison")
    if parse_bool(row["side_effect_committed"]) is not None:
        fail(f"{trial['trial_id']}: unavailable readback fabricated committed-effect knowledge")


def assert_final(
    row: dict[str, str],
    trial: dict[str, Any],
    mapped_case: dict[str, Any],
) -> None:
    if trial["observation_availability"] != "FULL":
        fail(f"{trial['trial_id']}: final authoritative phase created for unavailable source")
    assert_common_source(row, trial)
    source = mapped_case["source"]
    liminal = mapped_case["liminal"]
    expected = {
        "knowledge_state": "AUTHORITATIVE_READBACK",
        "readback_stage": "AUTHORITATIVE_SOURCE_READBACK",
        "external_outcome": source["external_outcome"],
        "observation_availability": source["observation_availability"],
        "liminal_execution": liminal["execution"],
        "liminal_response_integrity": liminal["response_integrity"],
        "liminal_causal_validity": liminal["causal_validity"],
        "liminal_continuity_posture": liminal["continuity_posture"],
    }
    for field, value in expected.items():
        if row[field] != value:
            fail(f"{trial['trial_id']}: final {field}={row[field]!r}, expected {value!r}")
    if parse_bool(row["externally_verified"]) is not source["externally_verified"]:
        fail(f"{trial['trial_id']}: final verification drift")
    if parse_int(row["effect_count"]) != source["effect_count"]:
        fail(f"{trial['trial_id']}: final effect-count drift")
    if parse_bool(row["digests_match_admission"]) is not source["digests_match_admission"]:
        fail(f"{trial['trial_id']}: final digest-match drift")
    if parse_bool(row["side_effect_committed"]) is not liminal["side_effect_committed"]:
        fail(f"{trial['trial_id']}: final committed-effect projection drift")


def verify(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    manifest = load_json(args.manifest)
    validate_inputs(fixture, manifest)

    cases = mapped_case_index(fixture)
    trials_by_action = {trial["action_id"]: trial for trial in manifest["trials"]}
    current = read_csv(args.current_csv)
    history = read_csv(args.history_csv)
    phase_a = read_csv(args.phase_a_asof_csv)
    phase_b = read_csv(args.phase_b_asof_csv)

    if len(current) != 18:
        fail(f"current view has {len(current)} rows, expected 18")
    if len(phase_a) != 18:
        fail(f"phase-A AS OF view has {len(phase_a)} rows, expected 18")
    if len(phase_b) != 18:
        fail(f"phase-B AS OF view has {len(phase_b)} rows, expected 18")
    if len(history) != 51:
        fail(f"history has {len(history)} rows, expected 51")

    for row in phase_a:
        trial = trials_by_action.get(row["_id"])
        if trial is None:
            fail(f"unknown action in phase-A view: {row['_id']}")
        assert_claim(row, trial)

    for row in phase_b:
        trial = trials_by_action.get(row["_id"])
        if trial is None:
            fail(f"unknown action in phase-B view: {row['_id']}")
        assert_unavailable(row, trial)

    for row in current:
        trial = trials_by_action.get(row["_id"])
        if trial is None:
            fail(f"unknown action in current view: {row['_id']}")
        if trial["observation_availability"] == "FULL":
            assert_final(row, trial, cases[trial["case"]])
        else:
            assert_unavailable(row, trial)

    grouped: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in history:
        grouped[row["_id"]].append(row)

    if set(grouped) != set(trials_by_action):
        fail("history action-id set differs from source action-id set")

    resolved = 0
    terminal_unknown = 0
    for action_id, versions in grouped.items():
        trial = trials_by_action[action_id]
        versions.sort(key=lambda row: row["_system_from"])
        expected_count = 3 if trial["observation_availability"] == "FULL" else 2
        if len(versions) != expected_count:
            fail(
                f"{trial['trial_id']}: expected {expected_count} system-time versions, "
                f"got {len(versions)}"
            )

        assert_claim(versions[0], trial)
        assert_unavailable(versions[1], trial)
        if expected_count == 3:
            assert_final(versions[2], trial, cases[trial["case"]])
            resolved += 1
        else:
            terminal_unknown += 1

        valid_from = versions[0]["_valid_from"]
        if not valid_from:
            fail(f"{trial['trial_id']}: missing valid-time anchor")
        for index, version in enumerate(versions):
            if version["_valid_from"] != valid_from:
                fail(f"{trial['trial_id']}: valid-time identity drift at version {index}")
            if not version["_system_from"]:
                fail(f"{trial['trial_id']}: missing system-time start at version {index}")
            if index + 1 < len(versions):
                nxt = versions[index + 1]
                if version["_system_from"] >= nxt["_system_from"]:
                    fail(f"{trial['trial_id']}: system-time order did not advance")
                if version["_system_to"] != nxt["_system_from"]:
                    fail(f"{trial['trial_id']}: system-time history is not contiguous")
            elif version["_system_to"]:
                fail(f"{trial['trial_id']}: current version unexpectedly has _system_to")

    if resolved != 15 or terminal_unknown != 3:
        fail(
            f"resolution counts drifted: resolved={resolved}, terminal_unknown={terminal_unknown}"
        )

    report = {
        "system_case": SYSTEM_CASE,
        "xtdb_version": XTDB_VERSION,
        "status": "PASS",
        "actions": len(trials_by_action),
        "history_versions": len(history),
        "temporarily_unavailable_then_resolved": resolved,
        "source_unavailable_remained_indeterminate": terminal_unknown,
        "invariants": [
            "the same pre-dispatch action_id is preserved across all knowledge revisions",
            "projection-level readback loss remains INDETERMINATE/REVALIDATE",
            "restored authoritative readback creates a later system-time version without erasing uncertainty history",
            "source-UNAVAILABLE trials never receive a fabricated authoritative phase",
            "valid-time admission identity remains stable across revisions",
            "final resolvable classifications equal the pinned LiminalDB mapping",
        ],
        "scope": (
            "counterfactual ingestion chronology over pinned evidence; not original event-time proof, "
            "distributed cross-database atomicity, or exactly-once external effects"
        ),
    }
    print(json.dumps(report, indent=2, sort_keys=True))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    prep = sub.add_parser("prepare")
    prep.add_argument("--fixture", required=True, type=Path)
    prep.add_argument("--manifest", required=True, type=Path)
    prep.add_argument("--phase-a-sql", required=True, type=Path)
    prep.add_argument("--phase-b-sql", required=True, type=Path)
    prep.add_argument("--phase-c-sql", required=True, type=Path)
    prep.set_defaults(func=prepare)

    check = sub.add_parser("verify")
    check.add_argument("--fixture", required=True, type=Path)
    check.add_argument("--manifest", required=True, type=Path)
    check.add_argument("--current-csv", required=True, type=Path)
    check.add_argument("--history-csv", required=True, type=Path)
    check.add_argument("--phase-a-asof-csv", required=True, type=Path)
    check.add_argument("--phase-b-asof-csv", required=True, type=Path)
    check.set_defaults(func=verify)

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
