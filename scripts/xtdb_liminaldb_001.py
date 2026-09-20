#!/usr/bin/env python3
"""Prepare and verify the XTDB-LIMINALDB-001 bitemporal projection experiment."""

from __future__ import annotations

import argparse
import csv
import json
from collections import defaultdict
from pathlib import Path
from typing import Any


SYSTEM_CASE = "XTDB-LIMINALDB-001"
TABLE = "crashpoint_xtdb_evidence"


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


def sql_text(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def sql_bool(value: bool | None) -> str:
    if value is None:
        return "NULL"
    return "TRUE" if value else "FALSE"


def sql_int(value: int | None) -> str:
    return "NULL" if value is None else str(value)


def sql_timestamp(value: str) -> str:
    normalized = value.replace("+00:00", "Z")
    return f"TIMESTAMP {sql_text(normalized)}"


def mapped_case_index(fixture: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {case["case_id"]: case for case in fixture["cases"]}


def validate_inputs(fixture: dict[str, Any], manifest: dict[str, Any]) -> None:
    if fixture.get("system_case") != "CRASHPOINT-LIMINALDB-001":
        fail("expected the pinned CRASHPOINT-LIMINALDB-001 mapping fixture")
    if manifest.get("status") != "COMPLETE" or manifest.get("all_agree") is not True:
        fail("Crashpoint manifest is not COMPLETE/all_agree")
    if manifest.get("trial_count") != 18:
        fail(f"expected 18 source trials, got {manifest.get('trial_count')!r}")

    cases = mapped_case_index(fixture)
    for trial in manifest["trials"]:
        case = cases.get(trial["case"])
        if case is None:
            fail(f"unmapped source case: {trial['case']}")
        source = case["source"]
        for field in (
            "client_claim",
            "external_outcome",
            "observation_availability",
            "externally_verified",
            "effect_count",
            "digests_match_admission",
        ):
            if trial.get(field) != source[field]:
                fail(
                    f"{trial['trial_id']}: source field {field}={trial.get(field)!r} "
                    f"does not match mapping fixture {source[field]!r}"
                )


def row_values(
    trial: dict[str, Any],
    mapped_case: dict[str, Any],
    phase: str,
    publication_commit: str,
) -> list[str]:
    liminal = mapped_case["liminal"]

    if phase == "claim":
        knowledge_state = "CLAIM_ONLY"
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
    elif phase == "readback":
        knowledge_state = (
            "READBACK_UNAVAILABLE"
            if trial["observation_availability"] == "UNAVAILABLE"
            else "AUTHORITATIVE_READBACK"
        )
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


COLUMNS = [
    "_id",
    "trial_id",
    "case_id",
    "knowledge_state",
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

    args.phase_a_sql.write_text(
        emit_insert(trials, cases, "claim", publication_commit),
        encoding="utf-8",
    )
    args.phase_b_sql.write_text(
        emit_insert(trials, cases, "readback", publication_commit),
        encoding="utf-8",
    )

    report = {
        "system_case": SYSTEM_CASE,
        "status": "PREPARED",
        "rows_per_phase": len(trials),
        "total_expected_system_versions": len(trials) * 2,
        "source_publication_commit": publication_commit,
        "semantic_boundary": (
            "Phase A stores only the client claim and remains INDETERMINATE/REVALIDATE. "
            "Phase B stores the published readback classification; unavailable readback "
            "remains INDETERMINATE/REVALIDATE."
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


def assert_phase_a(row: dict[str, str], trial: dict[str, Any]) -> None:
    prefix = trial["trial_id"]
    if row["knowledge_state"] != "CLAIM_ONLY":
        fail(f"{prefix}: phase-A knowledge_state drift")
    if row["client_claim"] != trial["client_claim"]:
        fail(f"{prefix}: phase-A client claim drift")
    if row["external_outcome"] != "INDETERMINATE":
        fail(f"{prefix}: client-only state upgraded external outcome")
    if row["observation_availability"] != "NOT_READ_BACK":
        fail(f"{prefix}: client-only state fabricated readback availability")
    if parse_bool(row["externally_verified"]) is not False:
        fail(f"{prefix}: client-only state marked externally verified")
    if parse_int(row["effect_count"]) is not None:
        fail(f"{prefix}: client-only state fabricated effect count")
    if row["liminal_continuity_posture"] != "REVALIDATE":
        fail(f"{prefix}: client-only state grants continuity beyond REVALIDATE")


def assert_final(
    row: dict[str, str],
    trial: dict[str, Any],
    mapped_case: dict[str, Any],
) -> None:
    prefix = trial["trial_id"]
    source = mapped_case["source"]
    liminal = mapped_case["liminal"]

    expected_knowledge = (
        "READBACK_UNAVAILABLE"
        if source["observation_availability"] == "UNAVAILABLE"
        else "AUTHORITATIVE_READBACK"
    )
    expected = {
        "knowledge_state": expected_knowledge,
        "client_claim": source["client_claim"],
        "external_outcome": source["external_outcome"],
        "observation_availability": source["observation_availability"],
        "liminal_execution": liminal["execution"],
        "liminal_response_integrity": liminal["response_integrity"],
        "liminal_causal_validity": liminal["causal_validity"],
        "liminal_continuity_posture": liminal["continuity_posture"],
    }
    for field, value in expected.items():
        if row[field] != value:
            fail(f"{prefix}: final {field}={row[field]!r}, expected {value!r}")

    if parse_bool(row["externally_verified"]) is not source["externally_verified"]:
        fail(f"{prefix}: final externally_verified drift")
    if parse_int(row["effect_count"]) != source["effect_count"]:
        fail(f"{prefix}: final effect_count drift")
    if parse_bool(row["digests_match_admission"]) is not source["digests_match_admission"]:
        fail(f"{prefix}: final digests_match_admission drift")
    if parse_bool(row["side_effect_committed"]) is not liminal["side_effect_committed"]:
        fail(f"{prefix}: final side_effect_committed drift")

    if source["observation_availability"] == "UNAVAILABLE":
        if row["external_outcome"] != "INDETERMINATE":
            fail(f"{prefix}: unavailable readback became a determinate external outcome")
        if row["liminal_continuity_posture"] != "REVALIDATE":
            fail(f"{prefix}: unavailable readback granted retry/continue permission")


def verify(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    manifest = load_json(args.manifest)
    validate_inputs(fixture, manifest)

    cases = mapped_case_index(fixture)
    trials_by_action = {trial["action_id"]: trial for trial in manifest["trials"]}

    current = read_csv(args.current_csv)
    history = read_csv(args.history_csv)
    asof = read_csv(args.asof_csv)

    if len(current) != 18:
        fail(f"current XTDB view has {len(current)} rows, expected 18")
    if len(asof) != 18:
        fail(f"phase-A system-time view has {len(asof)} rows, expected 18")
    if len(history) != 36:
        fail(f"bitemporal history has {len(history)} rows, expected 36")

    for row in current:
        trial = trials_by_action.get(row["_id"])
        if trial is None:
            fail(f"unknown action id in XTDB current view: {row['_id']}")
        assert_final(row, trial, cases[trial["case"]])

    for row in asof:
        trial = trials_by_action.get(row["_id"])
        if trial is None:
            fail(f"unknown action id in XTDB phase-A view: {row['_id']}")
        assert_phase_a(row, trial)

    grouped: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in history:
        grouped[row["_id"]].append(row)

    if set(grouped) != set(trials_by_action):
        fail("XTDB history action-id set differs from source action-id set")

    for action_id, versions in grouped.items():
        trial = trials_by_action[action_id]
        if len(versions) != 2:
            fail(f"{trial['trial_id']}: expected exactly 2 system-time versions")
        versions.sort(key=lambda row: row["_system_from"])
        first, second = versions
        assert_phase_a(first, trial)
        assert_final(second, trial, cases[trial["case"]])
        if not first["_system_from"] or not second["_system_from"]:
            fail(f"{trial['trial_id']}: missing system-time boundaries")
        if first["_system_from"] >= second["_system_from"]:
            fail(f"{trial['trial_id']}: system-time order did not advance")
        if first["_system_to"] != second["_system_from"]:
            fail(
                f"{trial['trial_id']}: first system version does not close at second version start"
            )
        if first["_valid_from"] != second["_valid_from"]:
            fail(f"{trial['trial_id']}: valid-time identity drifted across knowledge revisions")

    report = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "actions": len(trials_by_action),
        "system_versions": len(history),
        "phase_a_as_of_rows": len(asof),
        "current_rows": len(current),
        "invariants": [
            "client-only knowledge remains INDETERMINATE/REVALIDATE",
            "authoritative readback updates system-time without erasing the prior version",
            "valid-time identity is preserved across knowledge revisions",
            "UNAVAILABLE readback remains INDETERMINATE/REVALIDATE",
            "final XTDB classifications match the pinned LiminalDB mapping fixture",
        ],
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
    prep.set_defaults(func=prepare)

    check = sub.add_parser("verify")
    check.add_argument("--fixture", required=True, type=Path)
    check.add_argument("--manifest", required=True, type=Path)
    check.add_argument("--current-csv", required=True, type=Path)
    check.add_argument("--history-csv", required=True, type=Path)
    check.add_argument("--asof-csv", required=True, type=Path)
    check.set_defaults(func=verify)

    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
