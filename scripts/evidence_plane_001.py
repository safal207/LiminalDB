#!/usr/bin/env python3
"""Build and verify the EVIDENCE-PLANE-001 canonical evidence envelope."""

from __future__ import annotations

import argparse
import copy
import csv
import hashlib
import json
from collections import defaultdict
from datetime import datetime
from pathlib import Path
from typing import Any


SYSTEM_CASE = "EVIDENCE-PLANE-001"
SCHEMA = "liminaldb.evidence-envelope.v0.1"
SOURCE_CASE = "CRASHPOINT-LIMINALDB-001"


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def cases_by_id(fixture: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {case["case_id"]: case for case in fixture["cases"]}


def normalized_attempts(trial: dict[str, Any]) -> list[dict[str, Any]]:
    attempts: list[dict[str, Any]] = []
    if trial.get("worker_a_attempt_id"):
        attempts.append(
            {
                "attempt_id": trial["worker_a_attempt_id"],
                "worker": "A",
                "is_retry": False,
                "killed": trial.get("worker_a_killed"),
                "exit_status": trial.get("worker_a_exit_status"),
            }
        )
    if trial.get("worker_b_used"):
        if not trial.get("worker_b_attempt_id"):
            fail(f"{trial['trial_id']}: worker_b_used without worker_b_attempt_id")
        attempts.append(
            {
                "attempt_id": trial["worker_b_attempt_id"],
                "worker": "B",
                "is_retry": True,
                "killed": False,
                "exit_status": trial.get("worker_b_exit_status"),
            }
        )
    return attempts


def build_action(
    trial: dict[str, Any],
    mapped_case: dict[str, Any],
    publication_commit: str,
) -> dict[str, Any]:
    return {
        "action_id": trial["action_id"],
        "trial_id": trial["trial_id"],
        "case_id": trial["case"],
        "admission": {
            "admitted_at_utc": trial["admitted_at_utc"],
            "payload_digest": trial["admission_payload_digest"],
            "commit_confirmed": trial.get("admission_commit_confirmed"),
        },
        "claim": {
            "value": trial["client_claim"],
        },
        "attempts": normalized_attempts(trial),
        "observation": {
            "availability": trial["observation_availability"],
            "external_outcome": trial["external_outcome"],
            "externally_verified": trial["externally_verified"],
            "effect_count": trial["effect_count"],
            "digests_match_admission": trial["digests_match_admission"],
            "effect_attempt_ids": trial.get("effect_attempt_ids") or [],
            "effect_payload_digests": trial.get("effect_payload_digests") or [],
            "observer_raw_ledger_sha256": trial.get("observer_raw_ledger_sha256"),
        },
        "decision": copy.deepcopy(mapped_case["liminal"]),
        "provenance": {
            "source_publication_commit": publication_commit,
            "source_trial_id": trial["trial_id"],
        },
    }


def action_to_projection_trial(action: dict[str, Any]) -> dict[str, Any]:
    attempts = action["attempts"]
    worker_a = next((item for item in attempts if item["worker"] == "A"), None)
    worker_b = next((item for item in attempts if item["worker"] == "B"), None)
    observation = action["observation"]
    admission = action["admission"]
    return {
        "action_id": action["action_id"],
        "trial_id": action["trial_id"],
        "case": action["case_id"],
        "admitted_at_utc": admission["admitted_at_utc"],
        "admission_payload_digest": admission["payload_digest"],
        "client_claim": action["claim"]["value"],
        "external_outcome": observation["external_outcome"],
        "observation_availability": observation["availability"],
        "externally_verified": observation["externally_verified"],
        "effect_count": observation["effect_count"],
        "digests_match_admission": observation["digests_match_admission"],
        "effect_attempt_ids": observation["effect_attempt_ids"],
        "effect_payload_digests": observation["effect_payload_digests"],
        "worker_a_attempt_id": None if worker_a is None else worker_a["attempt_id"],
        "worker_a_killed": None if worker_a is None else worker_a["killed"],
        "worker_a_exit_status": None if worker_a is None else worker_a["exit_status"],
        "worker_b_used": worker_b is not None,
        "worker_b_attempt_id": None if worker_b is None else worker_b["attempt_id"],
        "worker_b_exit_status": None if worker_b is None else worker_b["exit_status"],
    }


def seal(envelope: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(envelope)
    result.pop("envelope_sha256", None)
    result["envelope_sha256"] = sha256_json(result)
    return result


def validate_action(action: dict[str, Any], mapped_case: dict[str, Any]) -> None:
    prefix = action["trial_id"]
    action_id = action["action_id"]
    if not action_id:
        fail(f"{prefix}: missing action_id")
    if action["case_id"] != mapped_case["case_id"]:
        fail(f"{prefix}: action case_id does not match case semantics")
    if action["provenance"]["source_trial_id"] != action["trial_id"]:
        fail(f"{prefix}: source trial identity drift")
    if action["admission"]["commit_confirmed"] is not True:
        fail(f"{prefix}: admission was not confirmed before dispatch")

    source = mapped_case["source"]
    observation = action["observation"]
    decision = action["decision"]

    expected_source = {
        "client_claim": action["claim"]["value"],
        "external_outcome": observation["external_outcome"],
        "observation_availability": observation["availability"],
        "externally_verified": observation["externally_verified"],
        "effect_count": observation["effect_count"],
        "digests_match_admission": observation["digests_match_admission"],
    }
    if expected_source != source:
        fail(f"{prefix}: observation/claim drift from frozen case semantics")
    if decision != mapped_case["liminal"]:
        fail(f"{prefix}: continuity semantics drift from frozen LiminalDB mapping")

    attempts = action["attempts"]
    attempt_ids = [item["attempt_id"] for item in attempts]
    if len(attempt_ids) != len(set(attempt_ids)):
        fail(f"{prefix}: duplicate attempt identity")
    for attempt_id in attempt_ids:
        if not attempt_id.startswith(action_id + ":"):
            fail(f"{prefix}: attempt {attempt_id} is not bound to action_id {action_id}")

    effect_attempt_ids = observation["effect_attempt_ids"]
    effect_payload_digests = observation["effect_payload_digests"]
    if len(effect_attempt_ids) != len(effect_payload_digests):
        fail(f"{prefix}: effect attempt/digest cardinality mismatch")
    for attempt_id in effect_attempt_ids:
        if attempt_id not in attempt_ids:
            fail(f"{prefix}: effect references foreign attempt {attempt_id}")

    effect_count = observation["effect_count"]
    if effect_count is None:
        if effect_attempt_ids:
            fail(f"{prefix}: unknown effect count still retains effect identities")
    elif effect_count != len(effect_attempt_ids):
        fail(
            f"{prefix}: effect_count={effect_count} but retained effects={len(effect_attempt_ids)}"
        )

    digest_match = observation["digests_match_admission"]
    admission_digest = action["admission"]["payload_digest"]
    if digest_match is True and any(
        digest != admission_digest for digest in effect_payload_digests
    ):
        fail(f"{prefix}: matching classification contains mismatched effect digest")
    if digest_match is False:
        if not effect_payload_digests:
            fail(f"{prefix}: mismatch classification has no retained effect digest")
        if all(digest == admission_digest for digest in effect_payload_digests):
            fail(f"{prefix}: mismatch classification contains only matching digests")

    if observation["availability"] == "UNAVAILABLE":
        if observation["external_outcome"] != "INDETERMINATE":
            fail(f"{prefix}: unavailable readback became determinate")
        if observation["externally_verified"] is not False:
            fail(f"{prefix}: unavailable readback marked externally verified")
        if observation["effect_count"] is not None:
            fail(f"{prefix}: unavailable readback fabricated effect count")
        if decision["continuity_posture"] != "REVALIDATE":
            fail(f"{prefix}: unavailable readback granted retry/continue authority")
        if decision["side_effect_committed"] is not None:
            fail(f"{prefix}: unavailable readback fabricated effect presence/absence")

    if observation["external_outcome"] == "NO_EFFECT":
        if observation["availability"] != "FULL":
            fail(f"{prefix}: NO_EFFECT without FULL readback")
        if observation["externally_verified"] is not True:
            fail(f"{prefix}: NO_EFFECT without external verification")
        if observation["effect_count"] != 0:
            fail(f"{prefix}: NO_EFFECT with nonzero/unknown effect count")
        if decision["continuity_posture"] != "RETRY_SIDE_EFFECT":
            fail(f"{prefix}: verified NO_EFFECT lost its bounded retry posture")


def validate_envelope(envelope: dict[str, Any]) -> None:
    if envelope.get("schema") != SCHEMA:
        fail(f"unexpected envelope schema: {envelope.get('schema')!r}")
    if envelope.get("system_case") != SYSTEM_CASE:
        fail(f"unexpected system case: {envelope.get('system_case')!r}")

    supplied_hash = envelope.get("envelope_sha256")
    unsigned = copy.deepcopy(envelope)
    unsigned.pop("envelope_sha256", None)
    actual_hash = sha256_json(unsigned)
    if supplied_hash != actual_hash:
        fail("envelope SHA-256 does not match canonical contents")

    actions = envelope.get("actions") or []
    if len(actions) != 18:
        fail(f"expected 18 actions, got {len(actions)}")

    action_ids = [item["action_id"] for item in actions]
    trial_ids = [item["trial_id"] for item in actions]
    if len(set(action_ids)) != len(action_ids):
        fail("duplicate action_id in canonical envelope")
    if len(set(trial_ids)) != len(trial_ids):
        fail("duplicate trial_id in canonical envelope")

    mapped = {case["case_id"]: case for case in envelope["case_semantics"]}
    for action in actions:
        case = mapped.get(action["case_id"])
        if case is None:
            fail(f"{action['trial_id']}: missing case semantics")
        validate_action(action, case)


def build(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    manifest = load_json(args.manifest)

    if fixture.get("system_case") != SOURCE_CASE:
        fail("expected CRASHPOINT-LIMINALDB-001 mapping fixture")
    if manifest.get("status") != "COMPLETE" or manifest.get("all_agree") is not True:
        fail("source manifest is not COMPLETE/all_agree")
    if manifest.get("trial_count") != 18:
        fail(f"expected 18 source trials, got {manifest.get('trial_count')!r}")

    mapped = cases_by_id(fixture)
    publication_commit = fixture["source"]["publication_commit"]
    actions: list[dict[str, Any]] = []

    for trial in sorted(manifest["trials"], key=lambda item: item["trial_id"]):
        case = mapped.get(trial["case"])
        if case is None:
            fail(f"{trial['trial_id']}: unmapped source case")
        for field in (
            "client_claim",
            "external_outcome",
            "observation_availability",
            "externally_verified",
            "effect_count",
            "digests_match_admission",
        ):
            source_value = case["source"][field]
            if trial.get(field) != source_value:
                fail(
                    f"{trial['trial_id']}: source {field}={trial.get(field)!r}, "
                    f"fixture expects {source_value!r}"
                )
        action = build_action(trial, case, publication_commit)
        validate_action(action, case)
        actions.append(action)

    envelope = seal(
        {
            "schema": SCHEMA,
            "system_case": SYSTEM_CASE,
            "source": {
                "repository": fixture["source"]["repository"],
                "publication_commit": publication_commit,
                "manifest_path": fixture["source"]["manifest_path"],
                "prediction_sha256": fixture["source"]["prediction_sha256"],
                "trial_count": 18,
                "manifest_sha256": sha256_json(manifest),
            },
            "mapping_contract": copy.deepcopy(fixture["mapping_contract"]),
            "case_semantics": copy.deepcopy(fixture["cases"]),
            "actions": actions,
            "claim_ceiling": (
                "The envelope preserves the pinned same-host Crashpoint evidence and "
                "LiminalDB mapping. It does not add target/recipient identity, distributed "
                "fencing, provider truth, or exactly-once guarantees."
            ),
        }
    )
    validate_envelope(envelope)
    write_json(args.envelope, envelope)

    projection_manifest = {
        "schema": "liminaldb.evidence-plane.projection-manifest.v0.1",
        "status": "COMPLETE",
        "all_agree": True,
        "trial_count": 18,
        "trials": [action_to_projection_trial(item) for item in actions],
    }
    write_json(args.projection_manifest, projection_manifest)

    projection_fixture = {
        "fixture_version": fixture["fixture_version"],
        "system_case": fixture["system_case"],
        "source": copy.deepcopy(fixture["source"]),
        "mapping_contract": copy.deepcopy(fixture["mapping_contract"]),
        "cases": copy.deepcopy(fixture["cases"]),
    }
    write_json(args.projection_fixture, projection_fixture)

    print(
        json.dumps(
            {
                "system_case": SYSTEM_CASE,
                "status": "PREPARED",
                "actions": len(actions),
                "envelope_sha256": envelope["envelope_sha256"],
                "projection_inputs": "derived from canonical envelope",
            },
            indent=2,
            sort_keys=True,
        )
    )


def negative_controls(args: argparse.Namespace) -> None:
    original = load_json(args.envelope)
    validate_envelope(original)
    results: list[dict[str, Any]] = []

    def expect_reject(name: str, candidate: dict[str, Any]) -> None:
        candidate = seal(candidate)
        try:
            validate_envelope(candidate)
        except SystemExit as exc:
            results.append({"control": name, "status": "REJECTED", "reason": str(exc)})
            return
        fail(f"negative control {name} was accepted")

    tampered = copy.deepcopy(original)
    tampered["actions"][0]["action_id"] = "tampered-action-id"
    expect_reject("action_id_vs_attempt_lineage", tampered)

    tampered = copy.deepcopy(original)
    clean = next(item for item in tampered["actions"] if item["case_id"] == "clean")
    clean["admission"]["payload_digest"] = "0" * 64
    expect_reject("admission_digest_vs_effect_digest", tampered)

    tampered = copy.deepcopy(original)
    unavailable = next(
        item for item in tampered["actions"] if item["case_id"] == "unavailable_readback"
    )
    unavailable["observation"].update(
        {
            "availability": "FULL",
            "external_outcome": "NO_EFFECT",
            "externally_verified": True,
            "effect_count": 0,
        }
    )
    unavailable["decision"]["continuity_posture"] = "RETRY_SIDE_EFFECT"
    unavailable["decision"]["side_effect_committed"] = False
    expect_reject("unavailable_must_not_become_no_effect", tampered)

    tampered = copy.deepcopy(original)
    clean = next(item for item in tampered["actions"] if item["case_id"] == "clean")
    clean["observation"]["effect_attempt_ids"] = []
    clean["observation"]["effect_payload_digests"] = []
    expect_reject("retained_effect_cardinality", tampered)

    report = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "controls": results,
        "count": len(results),
    }
    write_json(args.report, report)
    print(json.dumps(report, indent=2, sort_keys=True))


def compare_fixture(args: argparse.Namespace) -> None:
    envelope = load_json(args.envelope)
    validate_envelope(envelope)
    fixture = load_json(args.fixture)
    derived = {
        "fixture_version": fixture["fixture_version"],
        "system_case": fixture["system_case"],
        "source": copy.deepcopy(fixture["source"]),
        "mapping_contract": copy.deepcopy(envelope["mapping_contract"]),
        "cases": copy.deepcopy(envelope["case_semantics"]),
    }
    if canonical_bytes(derived) != canonical_bytes(fixture):
        fail("canonical envelope case semantics differ from repository LiminalDB fixture")
    print(
        json.dumps(
            {
                "system_case": SYSTEM_CASE,
                "status": "PASS",
                "liminal_fixture_equivalent": True,
            },
            indent=2,
            sort_keys=True,
        )
    )


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle, skipinitialspace=True))


def parse_timestamp(value: str) -> datetime:
    normalized = value.strip().replace(" ", "T")
    if normalized.endswith("Z"):
        normalized = normalized[:-1] + "+00:00"
    if normalized.endswith("+00"):
        normalized += ":00"
    return datetime.fromisoformat(normalized)


def expected_by_action(envelope: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {item["action_id"]: item for item in envelope["actions"]}


def verify_xtdb_current(
    rows: list[dict[str, str]],
    actions: dict[str, dict[str, Any]],
) -> None:
    if len(rows) != 18:
        fail(f"XTDB current projection has {len(rows)} rows, expected 18")
    for row in rows:
        action = actions.get(row["_id"])
        if action is None:
            fail(f"XTDB contains unknown action_id {row['_id']}")
        observation = action["observation"]
        decision = action["decision"]
        expected = {
            "trial_id": action["trial_id"],
            "case_id": action["case_id"],
            "client_claim": action["claim"]["value"],
            "external_outcome": observation["external_outcome"],
            "observation_availability": observation["availability"],
            "liminal_execution": decision["execution"],
            "liminal_response_integrity": decision["response_integrity"],
            "liminal_causal_validity": decision["causal_validity"],
            "liminal_continuity_posture": decision["continuity_posture"],
        }
        for field, value in expected.items():
            if row[field] != str(value):
                fail(
                    f"{action['trial_id']}: XTDB {field}={row[field]!r}, expected {value!r}"
                )


def verify_xtdb_history(
    rows: list[dict[str, str]],
    actions: dict[str, dict[str, Any]],
) -> None:
    if len(rows) != 36:
        fail(f"XTDB history has {len(rows)} rows, expected 36")
    grouped: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in rows:
        grouped[row["_id"]].append(row)
    if set(grouped) != set(actions):
        fail("XTDB history action-id set differs from canonical envelope")
    for action_id, versions in grouped.items():
        if len(versions) != 2:
            fail(f"{actions[action_id]['trial_id']}: expected exactly two system versions")
        versions.sort(key=lambda item: item["_system_from"])
        first, second = versions
        if first["knowledge_state"] != "CLAIM_ONLY":
            fail(f"{actions[action_id]['trial_id']}: first history version is not CLAIM_ONLY")
        if first["_valid_from"] != second["_valid_from"]:
            fail(f"{actions[action_id]['trial_id']}: valid-time anchor drifted")
        if first["_system_to"] != second["_system_from"]:
            fail(f"{actions[action_id]['trial_id']}: system-time versions are not contiguous")


def verify_bitemporal_witness(
    before: list[dict[str, str]],
    after: list[dict[str, str]],
    envelope: dict[str, Any],
) -> dict[str, Any]:
    if len(before) != 1 or len(after) != 1:
        fail("bitemporal witness queries must each return exactly one row")
    old = before[0]
    new = after[0]
    witness = next(item for item in envelope["actions"] if item["trial_id"] == "clean-0")
    if old["_id"] != witness["action_id"] or new["_id"] != witness["action_id"]:
        fail("bitemporal witness action identity drift")
    if old["knowledge_state"] != "CLAIM_ONLY":
        fail("witness phase A is not claim-only")
    if old["external_outcome"] != "INDETERMINATE":
        fail("witness phase A fabricated external truth")
    if old["liminal_continuity_posture"] != "REVALIDATE":
        fail("witness phase A granted continuity beyond revalidation")

    expected_observation = witness["observation"]
    expected_decision = witness["decision"]
    if new["external_outcome"] != expected_observation["external_outcome"]:
        fail("witness phase B external outcome drift")
    if new["liminal_continuity_posture"] != expected_decision["continuity_posture"]:
        fail("witness phase B continuity drift")
    if old["_valid_from"] != new["_valid_from"]:
        fail("witness valid-time anchor changed across knowledge revisions")

    valid_at = parse_timestamp(old["_valid_from"])
    old_system = parse_timestamp(old["_system_from"])
    new_system = parse_timestamp(new["_system_from"])
    if not (valid_at < old_system < new_system):
        fail(
            "expected source valid-time < claim projection system-time < "
            "authoritative-readback projection system-time"
        )

    return {
        "trial_id": witness["trial_id"],
        "action_id": witness["action_id"],
        "valid_from": old["_valid_from"],
        "phase_a_system_from": old["_system_from"],
        "phase_b_system_from": new["_system_from"],
        "phase_a": "CLAIM_ONLY / INDETERMINATE / REVALIDATE",
        "phase_b": (
            f"{new['knowledge_state']} / {new['external_outcome']} / "
            f"{new['liminal_continuity_posture']}"
        ),
    }


def verify_neo4j(
    rows: list[dict[str, str]],
    envelope: dict[str, Any],
) -> None:
    if len(rows) != 18:
        fail(f"Neo4j explanation projection has {len(rows)} rows, expected 18")
    expected = {item["trial_id"]: item for item in envelope["actions"]}
    for row in rows:
        action = expected.get(row["trial_id"])
        if action is None:
            fail(f"Neo4j contains unknown trial_id {row['trial_id']}")
        observation = action["observation"]
        decision = action["decision"]
        retained_effects = (
            0 if observation["effect_count"] is None else observation["effect_count"]
        )
        checks = {
            "case_id": action["case_id"],
            "client_claim": action["claim"]["value"],
            "availability": observation["availability"],
            "external_outcome": observation["external_outcome"],
            "continuity": decision["continuity_posture"],
        }
        for field, value in checks.items():
            if row[field] != str(value):
                fail(
                    f"{action['trial_id']}: Neo4j {field}={row[field]!r}, expected {value!r}"
                )
        if int(row["retained_effects"]) != retained_effects:
            fail(
                f"{action['trial_id']}: Neo4j retained_effects="
                f"{row['retained_effects']}, expected {retained_effects}"
            )


def verify(args: argparse.Namespace) -> None:
    envelope = load_json(args.envelope)
    validate_envelope(envelope)

    marker = args.liminal_marker.read_text(encoding="utf-8").strip()
    if marker != "PASS":
        fail("LiminalDB runtime mapping marker is not PASS")

    actions = expected_by_action(envelope)
    current = read_csv(args.xtdb_current)
    history = read_csv(args.xtdb_history)
    before = read_csv(args.xtdb_witness_before)
    after = read_csv(args.xtdb_witness_after)
    neo4j_rows = read_csv(args.neo4j_rows)

    verify_xtdb_current(current, actions)
    verify_xtdb_history(history, actions)
    witness = verify_bitemporal_witness(before, after, envelope)
    verify_neo4j(neo4j_rows, envelope)

    unavailable = [
        item
        for item in envelope["actions"]
        if item["observation"]["availability"] == "UNAVAILABLE"
    ]
    if len(unavailable) != 3:
        fail("expected three unavailable-readback controls")
    if any(
        item["decision"]["continuity_posture"] != "REVALIDATE"
        for item in unavailable
    ):
        fail("unavailable readback escaped REVALIDATE")

    report = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "actions": 18,
        "envelope_sha256": envelope["envelope_sha256"],
        "liminaldb_runtime_mapping": "PASS",
        "xtdb_projection": "PASS",
        "neo4j_projection": "PASS",
        "bitemporal_witness": witness,
        "canonical_invariant": (
            "Identity before execution; evidence before conclusion; temporal history "
            "without erasure; explanation without promoting graph absence to negative evidence."
        ),
    }
    write_json(args.report, report)
    print(json.dumps(report, indent=2, sort_keys=True))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    sub = root.add_subparsers(dest="command", required=True)

    build_cmd = sub.add_parser("build")
    build_cmd.add_argument("--fixture", type=Path, required=True)
    build_cmd.add_argument("--manifest", type=Path, required=True)
    build_cmd.add_argument("--envelope", type=Path, required=True)
    build_cmd.add_argument("--projection-manifest", type=Path, required=True)
    build_cmd.add_argument("--projection-fixture", type=Path, required=True)
    build_cmd.set_defaults(func=build)

    neg = sub.add_parser("negative-controls")
    neg.add_argument("--envelope", type=Path, required=True)
    neg.add_argument("--report", type=Path, required=True)
    neg.set_defaults(func=negative_controls)

    compare = sub.add_parser("compare-fixture")
    compare.add_argument("--envelope", type=Path, required=True)
    compare.add_argument("--fixture", type=Path, required=True)
    compare.set_defaults(func=compare_fixture)

    verify_cmd = sub.add_parser("verify")
    verify_cmd.add_argument("--envelope", type=Path, required=True)
    verify_cmd.add_argument("--liminal-marker", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-current", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-history", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-witness-before", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-witness-after", type=Path, required=True)
    verify_cmd.add_argument("--neo4j-rows", type=Path, required=True)
    verify_cmd.add_argument("--report", type=Path, required=True)
    verify_cmd.set_defaults(func=verify)

    return root


def main() -> None:
    args = parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
