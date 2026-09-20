#!/usr/bin/env python3
"""Prepare and verify the NEO4J-LIMINALDB-001 causal projection experiment."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


SYSTEM_CASE = "NEO4J-LIMINALDB-001"
NEO4J_VERSION = "2026.08.1"


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


def cypher_string(value: str) -> str:
    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"


def cypher_bool(value: bool) -> str:
    return "true" if value else "false"


def cypher_nullable_bool(value: bool | None) -> tuple[bool, str | None]:
    if value is None:
        return False, None
    return True, cypher_bool(value)


def cypher_int(value: int) -> str:
    return str(value)


def cases_by_id(fixture: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {case["case_id"]: case for case in fixture["cases"]}


def validate_inputs(fixture: dict[str, Any], manifest: dict[str, Any]) -> None:
    if fixture.get("system_case") != "CRASHPOINT-LIMINALDB-001":
        fail("expected CRASHPOINT-LIMINALDB-001 mapping fixture")
    if manifest.get("status") != "COMPLETE" or manifest.get("all_agree") is not True:
        fail("Crashpoint manifest is not COMPLETE/all_agree")
    if manifest.get("trial_count") != 18:
        fail(f"expected 18 source trials, got {manifest.get('trial_count')!r}")

    mapped = cases_by_id(fixture)
    for trial in manifest["trials"]:
        if trial["case"] not in mapped:
            fail(f"unmapped source case: {trial['case']}")


def prop_set(alias: str, values: dict[str, Any]) -> str:
    parts: list[str] = []
    for key, value in values.items():
        if value is None:
            continue
        if isinstance(value, bool):
            rendered = cypher_bool(value)
        elif isinstance(value, int):
            rendered = cypher_int(value)
        else:
            rendered = cypher_string(str(value))
        parts.append(f"{alias}.{key} = {rendered}")
    return ", ".join(parts)


def prepare(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    manifest = load_json(args.manifest)
    validate_inputs(fixture, manifest)
    mapped = cases_by_id(fixture)

    lines: list[str] = [
        "CREATE CONSTRAINT action_id_unique IF NOT EXISTS FOR (n:Action) REQUIRE n.action_id IS UNIQUE;",
        "CREATE CONSTRAINT authorization_id_unique IF NOT EXISTS FOR (n:Authorization) REQUIRE n.authorization_id IS UNIQUE;",
        "CREATE CONSTRAINT claim_id_unique IF NOT EXISTS FOR (n:Claim) REQUIRE n.claim_id IS UNIQUE;",
        "CREATE CONSTRAINT attempt_id_unique IF NOT EXISTS FOR (n:Attempt) REQUIRE n.attempt_id IS UNIQUE;",
        "CREATE CONSTRAINT observation_id_unique IF NOT EXISTS FOR (n:Observation) REQUIRE n.observation_id IS UNIQUE;",
        "CREATE CONSTRAINT effect_id_unique IF NOT EXISTS FOR (n:Effect) REQUIRE n.effect_id IS UNIQUE;",
        "CREATE CONSTRAINT continuity_id_unique IF NOT EXISTS FOR (n:Continuity) REQUIRE n.continuity_id IS UNIQUE;",
    ]

    expected_attempts = 0
    expected_effects = 0

    for trial in sorted(manifest["trials"], key=lambda item: item["trial_id"]):
        case = mapped[trial["case"]]
        liminal = case["liminal"]
        action_id = trial["action_id"]
        auth_id = f"{action_id}:authorization"
        claim_id = f"{action_id}:claim"
        observation_id = f"{action_id}:observation"
        continuity_id = f"{action_id}:continuity"

        effect_count_known = trial["effect_count"] is not None
        side_known, side_value = cypher_nullable_bool(liminal["side_effect_committed"])

        action_props: dict[str, Any] = {
            "trial_id": trial["trial_id"],
            "case_id": trial["case"],
            "admitted_at_utc": trial["admitted_at_utc"],
            "admission_payload_digest": trial["admission_payload_digest"],
            "source_publication_commit": fixture["source"]["publication_commit"],
            "effect_count_known": effect_count_known,
            "expected_effect_count": trial["effect_count"],
            "expected_continuity_posture": liminal["continuity_posture"],
        }
        lines.append(
            f"MERGE (a:Action {{action_id: {cypher_string(action_id)}}}) "
            f"SET {prop_set('a', action_props)};"
        )

        lines.append(
            f"MERGE (az:Authorization {{authorization_id: {cypher_string(auth_id)}}}) "
            f"SET {prop_set('az', {'payload_digest': trial['admission_payload_digest'], 'admitted_at_utc': trial['admitted_at_utc']})};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id: {cypher_string(action_id)}}}), "
            f"(az:Authorization {{authorization_id: {cypher_string(auth_id)}}}) "
            "MERGE (a)-[:HAS_AUTHORIZATION]->(az);"
        )

        lines.append(
            f"MERGE (cl:Claim {{claim_id: {cypher_string(claim_id)}}}) "
            f"SET {prop_set('cl', {'value': trial['client_claim']})};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id: {cypher_string(action_id)}}}), "
            f"(cl:Claim {{claim_id: {cypher_string(claim_id)}}}) "
            "MERGE (a)-[:HAS_CLAIM]->(cl);"
        )

        attempt_ids: list[tuple[str, str, bool, bool | None, int | None]] = []
        if trial.get("worker_a_attempt_id"):
            attempt_ids.append(
                (
                    trial["worker_a_attempt_id"],
                    "A",
                    False,
                    trial.get("worker_a_killed"),
                    trial.get("worker_a_exit_status"),
                )
            )
        if trial.get("worker_b_used"):
            if not trial.get("worker_b_attempt_id"):
                fail(f"{trial['trial_id']}: worker_b_used without worker_b_attempt_id")
            attempt_ids.append(
                (
                    trial["worker_b_attempt_id"],
                    "B",
                    True,
                    False,
                    trial.get("worker_b_exit_status"),
                )
            )
        expected_attempts += len(attempt_ids)

        known_attempt_ids = {item[0] for item in attempt_ids}
        for attempt_id, worker, is_retry, killed, exit_status in attempt_ids:
            attempt_props: dict[str, Any] = {
                "worker": worker,
                "is_retry": is_retry,
                "killed": killed,
                "exit_status": exit_status,
            }
            lines.append(
                f"MERGE (at:Attempt {{attempt_id: {cypher_string(attempt_id)}}}) "
                f"SET {prop_set('at', attempt_props)};"
            )
            lines.append(
                f"MATCH (a:Action {{action_id: {cypher_string(action_id)}}}), "
                f"(at:Attempt {{attempt_id: {cypher_string(attempt_id)}}}) "
                "MERGE (a)-[:HAS_ATTEMPT]->(at);"
            )

        obs_props: dict[str, Any] = {
            "external_outcome": trial["external_outcome"],
            "availability": trial["observation_availability"],
            "externally_verified": trial["externally_verified"],
            "effect_count_known": effect_count_known,
            "effect_count": trial["effect_count"],
            "digests_match_admission": trial["digests_match_admission"],
        }
        lines.append(
            f"MERGE (o:Observation {{observation_id: {cypher_string(observation_id)}}}) "
            f"SET {prop_set('o', obs_props)};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id: {cypher_string(action_id)}}}), "
            f"(o:Observation {{observation_id: {cypher_string(observation_id)}}}) "
            "MERGE (a)-[:HAS_OBSERVATION]->(o);"
        )
        lines.append(
            f"MATCH (cl:Claim {{claim_id: {cypher_string(claim_id)}}}), "
            f"(o:Observation {{observation_id: {cypher_string(observation_id)}}}) "
            "MERGE (cl)-[:EVALUATED_AGAINST]->(o);"
        )

        effect_attempt_ids = trial.get("effect_attempt_ids") or []
        effect_digests = trial.get("effect_payload_digests") or []
        if len(effect_attempt_ids) != len(effect_digests):
            fail(f"{trial['trial_id']}: effect attempt/digest length mismatch")
        if effect_count_known and len(effect_attempt_ids) != trial["effect_count"]:
            fail(f"{trial['trial_id']}: effect count does not match retained effects")
        if not effect_count_known and effect_attempt_ids:
            fail(f"{trial['trial_id']}: unavailable effect count still has retained effect ids")

        for index, (attempt_id, payload_digest) in enumerate(
            zip(effect_attempt_ids, effect_digests, strict=True)
        ):
            if attempt_id not in known_attempt_ids:
                fail(f"{trial['trial_id']}: effect references unknown attempt {attempt_id}")
            effect_id = f"{action_id}:effect:{index}"
            payload_matches = payload_digest == trial["admission_payload_digest"]
            lines.append(
                f"MERGE (e:Effect {{effect_id: {cypher_string(effect_id)}}}) "
                f"SET {prop_set('e', {'attempt_id': attempt_id, 'payload_digest': payload_digest, 'payload_matches_admission': payload_matches})};"
            )
            lines.append(
                f"MATCH (at:Attempt {{attempt_id: {cypher_string(attempt_id)}}}), "
                f"(e:Effect {{effect_id: {cypher_string(effect_id)}}}) "
                "MERGE (at)-[:PRODUCED]->(e);"
            )
            lines.append(
                f"MATCH (o:Observation {{observation_id: {cypher_string(observation_id)}}}), "
                f"(e:Effect {{effect_id: {cypher_string(effect_id)}}}) "
                "MERGE (o)-[:OBSERVES]->(e);"
            )
            expected_effects += 1

        continuity_props: dict[str, Any] = {
            "execution": liminal["execution"],
            "response_integrity": liminal["response_integrity"],
            "causal_validity": liminal["causal_validity"],
            "posture": liminal["continuity_posture"],
            "side_effect_committed_known": side_known,
        }
        if side_value is not None:
            continuity_props["side_effect_committed"] = liminal["side_effect_committed"]

        lines.append(
            f"MERGE (c:Continuity {{continuity_id: {cypher_string(continuity_id)}}}) "
            f"SET {prop_set('c', continuity_props)};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id: {cypher_string(action_id)}}}), "
            f"(c:Continuity {{continuity_id: {cypher_string(continuity_id)}}}) "
            "MERGE (a)-[:HAS_CONTINUITY]->(c);"
        )
        lines.append(
            f"MATCH (o:Observation {{observation_id: {cypher_string(observation_id)}}}), "
            f"(c:Continuity {{continuity_id: {cypher_string(continuity_id)}}}) "
            "MERGE (o)-[:INFORMS]->(c);"
        )

    args.load_cypher.write_text("\n".join(lines) + "\n", encoding="utf-8")

    checks: list[tuple[str, str, int]] = [
        ("action_nodes", "MATCH (n:Action) RETURN count(n)", 18),
        ("authorization_nodes", "MATCH (n:Authorization) RETURN count(n)", 18),
        ("claim_nodes", "MATCH (n:Claim) RETURN count(n)", 18),
        ("attempt_nodes", "MATCH (n:Attempt) RETURN count(n)", expected_attempts),
        ("observation_nodes", "MATCH (n:Observation) RETURN count(n)", 18),
        ("effect_nodes", "MATCH (n:Effect) RETURN count(n)", expected_effects),
        ("continuity_nodes", "MATCH (n:Continuity) RETURN count(n)", 18),
        (
            "complete_explanation_paths",
            "MATCH (a:Action)-[:HAS_AUTHORIZATION]->(:Authorization), "
            "(a)-[:HAS_CLAIM]->(cl:Claim)-[:EVALUATED_AGAINST]->(o:Observation), "
            "(a)-[:HAS_OBSERVATION]->(o)-[:INFORMS]->(c:Continuity), "
            "(a)-[:HAS_CONTINUITY]->(c) RETURN count(DISTINCT a)",
            18,
        ),
        (
            "effects_with_attempt_lineage",
            "MATCH (a:Action)-[:HAS_ATTEMPT]->(at:Attempt)-[:PRODUCED]->(e:Effect), "
            "(a)-[:HAS_OBSERVATION]->(o:Observation)-[:OBSERVES]->(e) "
            "WHERE at.attempt_id = e.attempt_id RETURN count(DISTINCT e)",
            expected_effects,
        ),
        (
            "effect_count_shape_violations",
            "MATCH (a:Action)-[:HAS_OBSERVATION]->(o:Observation) "
            "OPTIONAL MATCH (o)-[:OBSERVES]->(e:Effect) "
            "WITH a, o, count(e) AS actual "
            "WHERE a.effect_count_known = true AND actual <> a.expected_effect_count "
            "RETURN count(a)",
            0,
        ),
        (
            "continuity_mapping_violations",
            "MATCH (a:Action)-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE c.posture <> a.expected_continuity_posture RETURN count(a)",
            0,
        ),
        (
            "unavailable_readback_revalidate",
            "MATCH (a:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->(o:Observation), "
            "(a)-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE o.availability='UNAVAILABLE' AND o.external_outcome='INDETERMINATE' "
            "AND o.externally_verified=false AND o.effect_count_known=false "
            "AND c.posture='REVALIDATE' AND c.side_effect_committed_known=false "
            "RETURN count(a)",
            3,
        ),
        (
            "unavailable_readback_effect_nodes",
            "MATCH (:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->"
            "(:Observation)-[:OBSERVES]->(e:Effect) RETURN count(e)",
            0,
        ),
        (
            "verified_no_effect_retry",
            "MATCH (a:Action {case_id:'stopped_before_effect'})-[:HAS_OBSERVATION]->(o:Observation), "
            "(a)-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE o.availability='FULL' AND o.external_outcome='NO_EFFECT' "
            "AND o.externally_verified=true AND o.effect_count_known=true AND o.effect_count=0 "
            "AND c.posture='RETRY_SIDE_EFFECT' RETURN count(a)",
            3,
        ),
        (
            "verified_no_effect_effect_nodes",
            "MATCH (:Action {case_id:'stopped_before_effect'})-[:HAS_OBSERVATION]->"
            "(:Observation)-[:OBSERVES]->(e:Effect) RETURN count(e)",
            0,
        ),
        (
            "naive_retry_blocked_two_effects",
            "MATCH (a:Action {case_id:'naive_retry'}) "
            "OPTIONAL MATCH (a)-[:HAS_ATTEMPT]->(at:Attempt) "
            "WITH a, count(DISTINCT at) AS attempts "
            "MATCH (a)-[:HAS_OBSERVATION]->(o:Observation) "
            "OPTIONAL MATCH (o)-[:OBSERVES]->(e:Effect) "
            "WITH a, attempts, count(DISTINCT e) AS effects "
            "MATCH (a)-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE attempts=2 AND effects=2 AND c.posture='BLOCKED' RETURN count(a)",
            3,
        ),
        (
            "payload_mismatch_blocked",
            "MATCH (a:Action {case_id:'payload_mismatch'})-[:HAS_OBSERVATION]->"
            "(o:Observation)-[:OBSERVES]->(e:Effect), "
            "(a)-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE e.payload_matches_admission=false AND c.posture='BLOCKED' "
            "AND c.response_integrity='FAILED' AND c.causal_validity='INVALID' "
            "RETURN count(DISTINCT a)",
            3,
        ),
        (
            "lost_receipt_observed_effect",
            "MATCH (a:Action {case_id:'effect_before_lost_receipt'})-[:HAS_CLAIM]->"
            "(cl:Claim {value:'LOST'}), "
            "(a)-[:HAS_OBSERVATION]->(o:Observation)-[:OBSERVES]->(:Effect), "
            "(a)-[:HAS_CONTINUITY]->(c:Continuity {posture:'REPORT_ONLY'}) "
            "WHERE o.externally_verified=true AND o.external_outcome='ONE_EFFECT_MATCHING' "
            "RETURN count(DISTINCT a)",
            3,
        ),
    ]

    verify_lines: list[str] = []
    for name, query, expected in checks:
        verify_lines.append(
            f"CALL {{ {query} AS actual }} RETURN "
            f"{cypher_string(f'CHECK|{name}|')} + toString(actual) + "
            f"{cypher_string(f'|{expected}')} AS result;"
        )
    args.verify_cypher.write_text("\n".join(verify_lines) + "\n", encoding="utf-8")

    report = {
        "system_case": SYSTEM_CASE,
        "neo4j_version": NEO4J_VERSION,
        "status": "PREPARED",
        "actions": 18,
        "expected_attempts": expected_attempts,
        "expected_effects": expected_effects,
        "checks": len(checks),
        "boundary": (
            "Neo4j is a causal read-model only. LiminalDB remains authoritative for "
            "continuity semantics; absence of Effect nodes is interpreted only together "
            "with Observation availability/outcome."
        ),
    }
    print(json.dumps(report, indent=2, sort_keys=True))


def verify(args: argparse.Namespace) -> None:
    raw_lines = args.checks.read_text(encoding="utf-8").splitlines()
    seen: dict[str, tuple[int, int]] = {}
    for raw in raw_lines:
        line = raw.strip().strip('"')
        if "CHECK|" not in line:
            continue
        line = line[line.index("CHECK|") :]
        parts = line.split("|")
        if len(parts) != 4 or parts[0] != "CHECK":
            continue
        name = parts[1]
        actual = int(parts[2])
        expected = int(parts[3])
        if name in seen:
            fail(f"duplicate check result: {name}")
        seen[name] = (actual, expected)

    if not seen:
        fail("no CHECK records found in cypher-shell output")

    failures = {
        name: {"actual": actual, "expected": expected}
        for name, (actual, expected) in seen.items()
        if actual != expected
    }
    if failures:
        fail("graph invariants failed: " + json.dumps(failures, sort_keys=True))

    required = {
        "action_nodes",
        "complete_explanation_paths",
        "effects_with_attempt_lineage",
        "effect_count_shape_violations",
        "continuity_mapping_violations",
        "unavailable_readback_revalidate",
        "unavailable_readback_effect_nodes",
        "verified_no_effect_retry",
        "verified_no_effect_effect_nodes",
        "naive_retry_blocked_two_effects",
        "payload_mismatch_blocked",
        "lost_receipt_observed_effect",
    }
    missing = sorted(required - set(seen))
    if missing:
        fail(f"missing required checks: {missing}")

    report = {
        "system_case": SYSTEM_CASE,
        "neo4j_version": NEO4J_VERSION,
        "status": "PASS",
        "checks_passed": len(seen),
        "critical_distinction": (
            "stopped_before_effect and unavailable_readback both have zero Effect nodes, "
            "but remain distinguishable through Observation provenance and continuity posture."
        ),
        "invariants": [
            "all 18 actions have complete authorization/claim/observation/continuity explanation paths",
            "all retained effects have attempt lineage",
            "naive retry preserves two attempts and two effects and remains BLOCKED",
            "payload mismatch remains FAILED/INVALID/BLOCKED",
            "lost client receipt does not erase independently observed effect",
            "UNAVAILABLE readback remains INDETERMINATE/REVALIDATE",
            "verified NO_EFFECT remains a separate FULL-readback condition",
        ],
    }
    print(json.dumps(report, indent=2, sort_keys=True))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    sub = root.add_subparsers(dest="command", required=True)

    prep = sub.add_parser("prepare")
    prep.add_argument("--fixture", required=True, type=Path)
    prep.add_argument("--manifest", required=True, type=Path)
    prep.add_argument("--load-cypher", required=True, type=Path)
    prep.add_argument("--verify-cypher", required=True, type=Path)
    prep.set_defaults(func=prepare)

    check = sub.add_parser("verify")
    check.add_argument("--checks", required=True, type=Path)
    check.set_defaults(func=verify)
    return root


def main() -> None:
    args = parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
