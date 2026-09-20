#!/usr/bin/env python3
"""Bridge EXECUTION-RECEIPT-DURABILITY-001 into LiminalDB / XTDB / Neo4j."""

from __future__ import annotations

import argparse
import copy
import csv
import hashlib
import json
from collections import defaultdict
from pathlib import Path
from typing import Any

SYSTEM_CASE = "EVIDENCE-PLANE-DURABILITY-001"
SCHEMA = "liminaldb.evidence-plane-durability.v0.1"
XTDB_TABLE = "durability_evidence_plane"


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
    ).encode("utf-8")


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def sql_text(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def sql_bool(value: bool | None) -> str:
    if value is None:
        return "CAST(NULL AS BOOLEAN)"
    return "TRUE" if value else "FALSE"


def sql_int(value: int | None) -> str:
    return "CAST(NULL AS BIGINT)" if value is None else str(value)


def cypher_string(value: str) -> str:
    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"


def cypher_bool(value: bool) -> str:
    return "true" if value else "false"


def case_index(fixture: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {case["case_id"]: case for case in fixture["cases"]}


def validate_fixture(fixture: dict[str, Any]) -> None:
    if fixture.get("system_case") != SYSTEM_CASE:
        fail("unexpected mapping fixture system_case")
    source = fixture.get("source", {})
    if source.get("repository") != "safal207/ContractGraph-QA":
        fail("unexpected ContractGraph-QA source repository")
    if source.get("commit") != "aef57d1059513b0de1c7cc5945777949175b44de":
        fail("unexpected ContractGraph-QA source commit")
    if source.get("proof_system_case") != "EXECUTION-RECEIPT-DURABILITY-001":
        fail("unexpected durability proof system_case")
    cases = fixture.get("cases")
    if not isinstance(cases, list) or len(cases) != 3:
        fail("expected exactly three mapped durability cases")


def source_revision(recovery: dict[str, Any]) -> dict[str, Any]:
    return {
        "observation_availability": recovery["availability"],
        "external_outcome": recovery["external_outcome"],
        "externally_verified": recovery["externally_verified"],
        "effect_count": recovery["effect_count"],
        "recovery_decision": recovery["recovery_decision"],
    }


def validate_source_case(
    source_case: dict[str, Any],
    mapped_case: dict[str, Any],
) -> dict[str, Any]:
    case_id = mapped_case["case_id"]
    if source_case.get("case") != case_id:
        fail(f"{case_id}: source case identity drift")
    if source_case.get("passed") is not True:
        fail(f"{case_id}: source case is not PASS")
    if source_case.get("subject_returncode") != -9:
        fail(f"{case_id}: subject did not terminate with raw SIGKILL (-9)")

    action_id = source_case.get("action_id")
    target = source_case.get("target")
    payload_digest = source_case.get("payload_digest")
    if not all(isinstance(item, str) and item for item in (action_id, target, payload_digest)):
        fail(f"{case_id}: missing source identity fields")

    before = source_case["before_recovery"]
    after = source_case["after_recovery"]
    before_receipt = before["receipt"]
    before_receiver = before["receiver"]
    after_receiver = after["receiver"]

    pending_records = before_receipt.get("records") or []
    if len(pending_records) != 1:
        fail(f"{case_id}: expected exactly one pre-recovery PENDING")
    pending = pending_records[0]
    if pending.get("record_kind") != "PENDING":
        fail(f"{case_id}: durable source record is not PENDING")
    if pending.get("external_outcome") != "INDETERMINATE":
        fail(f"{case_id}: PENDING fabricated external outcome")
    if pending.get("externally_verified") is not False:
        fail(f"{case_id}: PENDING marked itself externally verified")
    if pending.get("effect_count") is not None:
        fail(f"{case_id}: PENDING fabricated effect count")
    if pending.get("target") != target or pending.get("payload_digest") != payload_digest:
        fail(f"{case_id}: PENDING identity does not bind target/payload")

    fresh_pending = before_receipt.get("fresh_pending_verifications") or []
    if (
        len(fresh_pending) != 1
        or fresh_pending[0].get("kind") != "FRESH_PROCESS_PENDING_READ"
        or fresh_pending[0].get("verified") is not True
        or fresh_pending[0].get("observed_record_sha256") != pending.get("record_sha256")
    ):
        fail(f"{case_id}: durable PENDING lacks independent fresh-process readback")

    if before_receiver.get("attempt_count") != mapped_case["expected_attempt_count"]:
        fail(f"{case_id}: provider attempt count drift before recovery")
    if before_receiver.get("effect_count") != mapped_case["expected_effect_count"]:
        fail(f"{case_id}: receiver effect count drift before recovery")
    if after_receiver.get("attempt_count") != mapped_case["expected_attempt_count"]:
        fail(f"{case_id}: recovery redispatched provider")
    if after_receiver.get("effect_count") != mapped_case["expected_effect_count"]:
        fail(f"{case_id}: recovery changed receiver effect cardinality")

    attempts = before_receiver.get("attempts") or []
    if len(attempts) != mapped_case["expected_attempt_count"]:
        fail(f"{case_id}: retained attempt rows drift")
    for attempt in attempts:
        if attempt.get("target") != target or attempt.get("payload_digest") != payload_digest:
            fail(f"{case_id}: attempt target/payload binding drift")

    effects = before_receiver.get("effects") or []
    if len(effects) != mapped_case["expected_effect_count"]:
        fail(f"{case_id}: retained effect rows drift")
    for effect in effects:
        if (
            effect.get("target") != target
            or effect.get("payload_digest") != payload_digest
            or effect.get("result") != "effect-ok"
        ):
            fail(f"{case_id}: effect target/payload/result binding drift")

    recoveries = source_case.get("recoveries") or []
    mapped_revisions = mapped_case["revisions"]
    if len(recoveries) != len(mapped_revisions):
        fail(
            f"{case_id}: source recoveries={len(recoveries)}, "
            f"fixture revisions={len(mapped_revisions)}"
        )

    revisions: list[dict[str, Any]] = []
    for index, (recovery, mapped) in enumerate(
        zip(recoveries, mapped_revisions, strict=True),
        start=1,
    ):
        observed = source_revision(recovery)
        if observed != mapped["source"]:
            fail(
                f"{case_id}: revision {index} source classification drift: "
                + json.dumps({"observed": observed, "expected": mapped["source"]}, sort_keys=True)
            )
        revisions.append(
            {
                "revision": index,
                "source": copy.deepcopy(observed),
                "liminal": copy.deepcopy(mapped["liminal"]),
                "reconciliation_record_sha256": recovery["reconciliation_record_sha256"],
                "readback_used": recovery["readback_used"],
            }
        )

    if case_id == "unavailable_readback":
        first = revisions[0]
        if first["source"]["observation_availability"] != "UNAVAILABLE":
            fail("unavailable_readback: first revision is not UNAVAILABLE")
        if first["source"]["effect_count"] is not None:
            fail("unavailable_readback: first revision leaked hidden effect count")
        if first["readback_used"] is not False:
            fail("unavailable_readback: hidden receiver truth was used during UNAVAILABLE")
        if mapped_case["expected_effect_count"] != 1:
            fail("unavailable_readback: fixture must retain one hidden receiver effect")

    return {
        "action_id": action_id,
        "case_id": case_id,
        "target": target,
        "payload_digest": payload_digest,
        "correlation": {
            "trace_id": pending["trace_id"],
            "decision_id": pending["decision_id"],
            "execution_id": pending["execution_id"],
            "evidence_id": pending["evidence_id"],
        },
        "process_crash": {
            "signal": "SIGKILL",
            "returncode": -9,
        },
        "pending": {
            "record_sha256": pending["record_sha256"],
            "fresh_process_verified": True,
            "observation_availability": pending["observation_availability"],
            "external_outcome": pending["external_outcome"],
        },
        "receiver_truth": {
            "attempt_count": mapped_case["expected_attempt_count"],
            "effect_count": mapped_case["expected_effect_count"],
            "effects": copy.deepcopy(effects),
            "fresh_effect_verifications": copy.deepcopy(
                before_receiver.get("fresh_effect_verifications") or []
            ),
        },
        "revisions": revisions,
        "final_liminal": copy.deepcopy(revisions[-1]["liminal"]),
    }


def validate_source_report(
    source_report: dict[str, Any],
    fixture: dict[str, Any],
) -> list[dict[str, Any]]:
    if source_report.get("system_case") != "EXECUTION-RECEIPT-DURABILITY-001":
        fail("source report is not EXECUTION-RECEIPT-DURABILITY-001")
    if source_report.get("status") != "PASS":
        fail("source durability report is not PASS")
    if source_report.get("barrier") != "durable_pending_then_real_process_sigkill_before_terminal":
        fail("source durability barrier drift")
    runtime = source_report.get("source_runtime", {})
    if runtime.get("repository") != fixture["source"]["runtime_subject_repository"]:
        fail("source runtime repository drift")
    if runtime.get("commit") != fixture["source"]["runtime_subject_commit"]:
        fail("source runtime commit drift")

    mapped = case_index(fixture)
    source_cases = source_report.get("cases") or {}
    if set(source_cases) != set(mapped):
        fail("source durability case set differs from mapping fixture")

    actions = [
        validate_source_case(source_cases[case_id], mapped[case_id])
        for case_id in sorted(mapped)
    ]
    action_ids = [action["action_id"] for action in actions]
    if len(set(action_ids)) != len(action_ids):
        fail("duplicate action_id across durability cases")
    return actions


def build_envelope(
    source_report_path: Path,
    fixture: dict[str, Any],
) -> dict[str, Any]:
    source_report = load_json(source_report_path)
    actions = validate_source_report(source_report, fixture)

    envelope = {
        "schema": SCHEMA,
        "system_case": SYSTEM_CASE,
        "source": {
            "repository": fixture["source"]["repository"],
            "commit": fixture["source"]["commit"],
            "proof_system_case": fixture["source"]["proof_system_case"],
            "source_report_sha256": sha256_file(source_report_path),
            "runtime_subject_repository": fixture["source"]["runtime_subject_repository"],
            "runtime_subject_commit": fixture["source"]["runtime_subject_commit"],
        },
        "mapping_contract": copy.deepcopy(fixture["mapping_contract"]),
        "temporal_claim": (
            "XTDB stores system-time knowledge revisions from this bridge run. "
            "The source durability proof does not carry authoritative world-event timestamps, "
            "so this bridge makes no new valid-time claim."
        ),
        "actions": actions,
        "claim_ceiling": fixture["mapping_contract"]["claim_ceiling"],
    }
    unsigned = copy.deepcopy(envelope)
    envelope["envelope_sha256"] = sha256_json(unsigned)
    return envelope


XTDB_COLUMNS = [
    "_id",
    "case_id",
    "target",
    "payload_digest",
    "knowledge_state",
    "revision",
    "observation_availability",
    "external_outcome",
    "externally_verified",
    "effect_count",
    "liminal_authority",
    "liminal_execution",
    "liminal_response_integrity",
    "liminal_causal_validity",
    "liminal_continuity_posture",
    "side_effect_committed",
    "source_recovery_decision",
    "source_commit",
    "envelope_sha256",
]


def xtdb_row(
    action: dict[str, Any],
    *,
    knowledge_state: str,
    revision: int,
    source: dict[str, Any],
    liminal: dict[str, Any] | None,
    source_commit: str,
    envelope_sha256: str,
) -> str:
    if liminal is None:
        authority = "NOT_EVALUATED"
        execution = "NOT_OBSERVED"
        response_integrity = "UNKNOWN"
        causal_validity = "NOT_EVALUATED"
        continuity = "NOT_EVALUATED"
        side_effect = None
    else:
        authority = liminal["authority"]
        execution = liminal["execution"]
        response_integrity = liminal["response_integrity"]
        causal_validity = liminal["causal_validity"]
        continuity = liminal["continuity_posture"]
        side_effect = liminal["side_effect_committed"]

    values = [
        sql_text(action["action_id"]),
        sql_text(action["case_id"]),
        sql_text(action["target"]),
        sql_text(action["payload_digest"]),
        sql_text(knowledge_state),
        str(revision),
        sql_text(source["observation_availability"]),
        sql_text(source["external_outcome"]),
        sql_bool(source["externally_verified"]),
        sql_int(source["effect_count"]),
        sql_text(authority),
        sql_text(execution),
        sql_text(response_integrity),
        sql_text(causal_validity),
        sql_text(continuity),
        sql_bool(side_effect),
        sql_text(source["recovery_decision"]),
        sql_text(source_commit),
        sql_text(envelope_sha256),
    ]
    return "(" + ", ".join(values) + ")"


def emit_xtdb_phase(envelope: dict[str, Any], phase: str) -> str:
    rows: list[str] = []
    for action in envelope["actions"]:
        if phase == "pending":
            source = {
                "observation_availability": "NOT_READ_BACK",
                "external_outcome": "INDETERMINATE",
                "externally_verified": False,
                "effect_count": None,
                "recovery_decision": "AWAIT_TERMINAL_OR_READBACK",
            }
            rows.append(
                xtdb_row(
                    action,
                    knowledge_state="PENDING_DURABLE",
                    revision=0,
                    source=source,
                    liminal=None,
                    source_commit=envelope["source"]["commit"],
                    envelope_sha256=envelope["envelope_sha256"],
                )
            )
        elif phase == "revision1":
            revision = action["revisions"][0]
            rows.append(
                xtdb_row(
                    action,
                    knowledge_state=(
                        "READBACK_UNAVAILABLE"
                        if revision["source"]["observation_availability"] == "UNAVAILABLE"
                        else "READBACK_FULL"
                    ),
                    revision=1,
                    source=revision["source"],
                    liminal=revision["liminal"],
                    source_commit=envelope["source"]["commit"],
                    envelope_sha256=envelope["envelope_sha256"],
                )
            )
        elif phase == "revision2":
            if len(action["revisions"]) < 2:
                continue
            revision = action["revisions"][1]
            rows.append(
                xtdb_row(
                    action,
                    knowledge_state="READBACK_FULL",
                    revision=2,
                    source=revision["source"],
                    liminal=revision["liminal"],
                    source_commit=envelope["source"]["commit"],
                    envelope_sha256=envelope["envelope_sha256"],
                )
            )
        else:
            raise AssertionError(phase)

    if not rows:
        return "-- no rows in this phase\n"
    return (
        f"INSERT INTO {XTDB_TABLE} (" + ", ".join(XTDB_COLUMNS) + ") VALUES\n  "
        + ",\n  ".join(rows)
        + ";\n"
    )


def cypher_prop_set(alias: str, props: dict[str, Any]) -> str:
    parts: list[str] = []
    for key, value in props.items():
        if value is None:
            continue
        if isinstance(value, bool):
            rendered = cypher_bool(value)
        elif isinstance(value, int):
            rendered = str(value)
        else:
            rendered = cypher_string(str(value))
        parts.append(f"{alias}.{key} = {rendered}")
    return ", ".join(parts)


def build_neo4j(envelope: dict[str, Any]) -> tuple[str, str]:
    lines = [
        "CREATE CONSTRAINT durability_action_unique IF NOT EXISTS FOR (n:Action) REQUIRE n.action_id IS UNIQUE;",
        "CREATE CONSTRAINT durability_pending_unique IF NOT EXISTS FOR (n:PendingReceipt) REQUIRE n.pending_id IS UNIQUE;",
        "CREATE CONSTRAINT durability_attempt_unique IF NOT EXISTS FOR (n:Attempt) REQUIRE n.attempt_id IS UNIQUE;",
        "CREATE CONSTRAINT durability_effect_unique IF NOT EXISTS FOR (n:Effect) REQUIRE n.effect_id IS UNIQUE;",
        "CREATE CONSTRAINT durability_observation_unique IF NOT EXISTS FOR (n:Observation) REQUIRE n.observation_id IS UNIQUE;",
        "CREATE CONSTRAINT durability_continuity_unique IF NOT EXISTS FOR (n:Continuity) REQUIRE n.continuity_id IS UNIQUE;",
    ]

    for action in envelope["actions"]:
        action_id = action["action_id"]
        action_props = {
            "case_id": action["case_id"],
            "target": action["target"],
            "payload_digest": action["payload_digest"],
            "attempt_count": action["receiver_truth"]["attempt_count"],
            "receiver_effect_count": action["receiver_truth"]["effect_count"],
            "source_commit": envelope["source"]["commit"],
            "envelope_sha256": envelope["envelope_sha256"],
        }
        lines.append(
            f"MERGE (a:Action {{action_id:{cypher_string(action_id)}}}) "
            f"SET {cypher_prop_set('a', action_props)};"
        )

        pending_id = f"{action_id}:pending"
        lines.append(
            f"MERGE (p:PendingReceipt {{pending_id:{cypher_string(pending_id)}}}) "
            f"SET {cypher_prop_set('p', {'record_sha256': action['pending']['record_sha256'], 'fresh_process_verified': True, 'external_outcome': 'INDETERMINATE'})};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id:{cypher_string(action_id)}}}), "
            f"(p:PendingReceipt {{pending_id:{cypher_string(pending_id)}}}) "
            "MERGE (a)-[:HAS_PENDING]->(p);"
        )

        attempt_id = f"{action_id}:attempt:1"
        lines.append(
            f"MERGE (at:Attempt {{attempt_id:{cypher_string(attempt_id)}}}) "
            f"SET {cypher_prop_set('at', {'target': action['target'], 'payload_digest': action['payload_digest']})};"
        )
        lines.append(
            f"MATCH (a:Action {{action_id:{cypher_string(action_id)}}}), "
            f"(at:Attempt {{attempt_id:{cypher_string(attempt_id)}}}) "
            "MERGE (a)-[:HAS_ATTEMPT]->(at);"
        )

        effect_id: str | None = None
        if action["receiver_truth"]["effect_count"] == 1:
            effect_id = f"{action_id}:effect:1"
            lines.append(
                f"MERGE (e:Effect {{effect_id:{cypher_string(effect_id)}}}) "
                f"SET {cypher_prop_set('e', {'target': action['target'], 'payload_digest': action['payload_digest'], 'result': 'effect-ok', 'receiver_truth': True})};"
            )
            lines.append(
                f"MATCH (at:Attempt {{attempt_id:{cypher_string(attempt_id)}}}), "
                f"(e:Effect {{effect_id:{cypher_string(effect_id)}}}) "
                "MERGE (at)-[:PRODUCED]->(e);"
            )

        previous_observation: str | None = None
        for revision in action["revisions"]:
            index = revision["revision"]
            observation_id = f"{action_id}:observation:{index}"
            continuity_id = f"{action_id}:continuity:{index}"
            source = revision["source"]
            liminal = revision["liminal"]

            lines.append(
                f"MERGE (o:Observation {{observation_id:{cypher_string(observation_id)}}}) "
                f"SET {cypher_prop_set('o', {'revision': index, 'availability': source['observation_availability'], 'external_outcome': source['external_outcome'], 'externally_verified': source['externally_verified'], 'effect_count_known': source['effect_count'] is not None, 'effect_count': source['effect_count'], 'recovery_decision': source['recovery_decision']})};"
            )
            lines.append(
                f"MATCH (a:Action {{action_id:{cypher_string(action_id)}}}), "
                f"(o:Observation {{observation_id:{cypher_string(observation_id)}}}) "
                "MERGE (a)-[:HAS_OBSERVATION]->(o);"
            )

            if previous_observation is not None:
                lines.append(
                    f"MATCH (old:Observation {{observation_id:{cypher_string(previous_observation)}}}), "
                    f"(new:Observation {{observation_id:{cypher_string(observation_id)}}}) "
                    "MERGE (old)-[:NEXT_KNOWLEDGE]->(new);"
                )
            previous_observation = observation_id

            if (
                effect_id is not None
                and source["observation_availability"] == "FULL"
                and source["effect_count"] == 1
            ):
                lines.append(
                    f"MATCH (o:Observation {{observation_id:{cypher_string(observation_id)}}}), "
                    f"(e:Effect {{effect_id:{cypher_string(effect_id)}}}) "
                    "MERGE (o)-[:OBSERVES]->(e);"
                )

            continuity_props = {
                "revision": index,
                "authority": liminal["authority"],
                "execution": liminal["execution"],
                "response_integrity": liminal["response_integrity"],
                "causal_validity": liminal["causal_validity"],
                "posture": liminal["continuity_posture"],
                "side_effect_committed_known": liminal["side_effect_committed"] is not None,
                "side_effect_committed": liminal["side_effect_committed"],
                "source_recovery_decision": source["recovery_decision"],
            }
            lines.append(
                f"MERGE (c:Continuity {{continuity_id:{cypher_string(continuity_id)}}}) "
                f"SET {cypher_prop_set('c', continuity_props)};"
            )
            lines.append(
                f"MATCH (a:Action {{action_id:{cypher_string(action_id)}}}), "
                f"(c:Continuity {{continuity_id:{cypher_string(continuity_id)}}}) "
                "MERGE (a)-[:HAS_CONTINUITY]->(c);"
            )
            lines.append(
                f"MATCH (o:Observation {{observation_id:{cypher_string(observation_id)}}}), "
                f"(c:Continuity {{continuity_id:{cypher_string(continuity_id)}}}) "
                "MERGE (o)-[:INFORMS]->(c);"
            )

    checks: list[tuple[str, str, int]] = [
        ("actions", "MATCH (n:Action) RETURN count(n)", 3),
        ("pending", "MATCH (n:PendingReceipt) RETURN count(n)", 3),
        ("attempts", "MATCH (n:Attempt) RETURN count(n)", 3),
        ("effects", "MATCH (n:Effect) RETURN count(n)", 2),
        ("observations", "MATCH (n:Observation) RETURN count(n)", 4),
        ("continuities", "MATCH (n:Continuity) RETURN count(n)", 4),
        (
            "attempt_cardinality_violations",
            "MATCH (a:Action) OPTIONAL MATCH (a)-[:HAS_ATTEMPT]->(at:Attempt) "
            "WITH a, count(at) AS n WHERE n <> 1 RETURN count(a)",
            0,
        ),
        (
            "unavailable_hidden_effect_exists",
            "MATCH (a:Action {case_id:'unavailable_readback'})-[:HAS_ATTEMPT]->(:Attempt)-[:PRODUCED]->(e:Effect) RETURN count(e)",
            1,
        ),
        (
            "unavailable_first_observes_effect",
            "MATCH (:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->(o:Observation {revision:1})-[:OBSERVES]->(e:Effect) RETURN count(e)",
            0,
        ),
        (
            "unavailable_first_revalidate",
            "MATCH (:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->"
            "(o:Observation {revision:1})-[:INFORMS]->(c:Continuity {revision:1}) "
            "WHERE o.availability='UNAVAILABLE' AND o.external_outcome='INDETERMINATE' "
            "AND c.authority='REVALIDATION_REQUIRED' AND c.posture='REVALIDATE' RETURN count(c)",
            1,
        ),
        (
            "unavailable_second_observes_effect",
            "MATCH (:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->"
            "(o:Observation {revision:2})-[:OBSERVES]->(e:Effect) "
            "WHERE o.availability='FULL' AND o.external_outcome='ONE_EFFECT_MATCHING' RETURN count(e)",
            1,
        ),
        (
            "unavailable_second_report_only",
            "MATCH (:Action {case_id:'unavailable_readback'})-[:HAS_OBSERVATION]->"
            "(:Observation {revision:2})-[:INFORMS]->(c:Continuity {revision:2}) "
            "WHERE c.authority='CONSUMED' AND c.posture='REPORT_ONLY' RETURN count(c)",
            1,
        ),
        (
            "crash_before_no_effect_nodes",
            "MATCH (:Action {case_id:'crash_before_effect'})-[:HAS_ATTEMPT]->(:Attempt)-[:PRODUCED]->(e:Effect) RETURN count(e)",
            0,
        ),
        (
            "crash_before_revalidate",
            "MATCH (:Action {case_id:'crash_before_effect'})-[:HAS_OBSERVATION]->"
            "(o:Observation)-[:INFORMS]->(c:Continuity) "
            "WHERE o.external_outcome='NO_EFFECT' AND c.authority='REVALIDATION_REQUIRED' "
            "AND c.posture='REVALIDATE' RETURN count(c)",
            1,
        ),
        (
            "effect_then_finalizes_without_redispatch",
            "MATCH (:Action {case_id:'effect_then_crash'})-[:HAS_OBSERVATION]->"
            "(o:Observation)-[:OBSERVES]->(e:Effect), "
            "(:Action {case_id:'effect_then_crash'})-[:HAS_CONTINUITY]->(c:Continuity) "
            "WHERE o.external_outcome='ONE_EFFECT_MATCHING' AND c.authority='CONSUMED' "
            "AND c.posture='REPORT_ONLY' RETURN count(DISTINCT c)",
            1,
        ),
    ]

    verify_lines = [
        f"CALL {{ {query} AS actual }} RETURN "
        f"{cypher_string('CHECK|' + name + '|')} + toString(actual) + {cypher_string('|' + str(expected))} AS result;"
        for name, query, expected in checks
    ]
    return "\n".join(lines) + "\n", "\n".join(verify_lines) + "\n"


def build(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    validate_fixture(fixture)
    envelope = build_envelope(args.source_report, fixture)
    write_json(args.envelope, envelope)

    args.xtdb_pending_sql.write_text(
        emit_xtdb_phase(envelope, "pending"),
        encoding="utf-8",
    )
    args.xtdb_revision1_sql.write_text(
        emit_xtdb_phase(envelope, "revision1"),
        encoding="utf-8",
    )
    args.xtdb_revision2_sql.write_text(
        emit_xtdb_phase(envelope, "revision2"),
        encoding="utf-8",
    )

    load_cypher, verify_cypher = build_neo4j(envelope)
    args.neo4j_load_cypher.write_text(load_cypher, encoding="utf-8")
    args.neo4j_verify_cypher.write_text(verify_cypher, encoding="utf-8")

    print(
        json.dumps(
            {
                "system_case": SYSTEM_CASE,
                "status": "PREPARED",
                "actions": len(envelope["actions"]),
                "knowledge_versions_expected": 7,
                "receiver_effect_nodes_expected": 2,
                "envelope_sha256": envelope["envelope_sha256"],
                "source_report_sha256": envelope["source"]["source_report_sha256"],
            },
            indent=2,
            sort_keys=True,
        )
    )


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle))


def parse_bool(value: str) -> bool | None:
    value = value.strip().lower()
    if value == "":
        return None
    if value in {"t", "true"}:
        return True
    if value in {"f", "false"}:
        return False
    fail(f"unexpected boolean CSV value: {value!r}")


def parse_int(value: str) -> int | None:
    value = value.strip()
    return None if value == "" else int(value)


def verify_xtdb(
    envelope: dict[str, Any],
    current_rows: list[dict[str, str]],
    history_rows: list[dict[str, str]],
) -> dict[str, Any]:
    if len(current_rows) != 3:
        fail(f"XTDB current rows={len(current_rows)}, expected 3")
    if len(history_rows) != 7:
        fail(f"XTDB history rows={len(history_rows)}, expected 7")

    actions = {action["action_id"]: action for action in envelope["actions"]}

    for row in current_rows:
        action = actions.get(row["_id"])
        if action is None:
            fail(f"XTDB contains unknown action_id {row['_id']}")
        final = action["revisions"][-1]
        expected = {
            "case_id": action["case_id"],
            "target": action["target"],
            "payload_digest": action["payload_digest"],
            "external_outcome": final["source"]["external_outcome"],
            "observation_availability": final["source"]["observation_availability"],
            "liminal_authority": final["liminal"]["authority"],
            "liminal_execution": final["liminal"]["execution"],
            "liminal_response_integrity": final["liminal"]["response_integrity"],
            "liminal_causal_validity": final["liminal"]["causal_validity"],
            "liminal_continuity_posture": final["liminal"]["continuity_posture"],
            "source_recovery_decision": final["source"]["recovery_decision"],
            "source_commit": envelope["source"]["commit"],
            "envelope_sha256": envelope["envelope_sha256"],
        }
        for field, value in expected.items():
            if row[field] != str(value):
                fail(
                    f"{action['case_id']}: XTDB current {field}={row[field]!r}, expected={value!r}"
                )
        if parse_bool(row["externally_verified"]) is not final["source"]["externally_verified"]:
            fail(f"{action['case_id']}: XTDB externally_verified drift")
        if parse_int(row["effect_count"]) != final["source"]["effect_count"]:
            fail(f"{action['case_id']}: XTDB effect_count drift")

    grouped: dict[str, list[dict[str, str]]] = defaultdict(list)
    for row in history_rows:
        grouped[row["_id"]].append(row)

    expected_sequences = {
        action["action_id"]: (
            ["PENDING_DURABLE", "READBACK_FULL"]
            if len(action["revisions"]) == 1
            else ["PENDING_DURABLE", "READBACK_UNAVAILABLE", "READBACK_FULL"]
        )
        for action in envelope["actions"]
    }
    for action_id, rows in grouped.items():
        rows.sort(key=lambda item: item["_system_from"])
        sequence = [row["knowledge_state"] for row in rows]
        if sequence != expected_sequences[action_id]:
            fail(
                f"{actions[action_id]['case_id']}: XTDB knowledge sequence={sequence}, "
                f"expected={expected_sequences[action_id]}"
            )

    unavailable = next(
        action for action in envelope["actions"] if action["case_id"] == "unavailable_readback"
    )
    unavailable_rows = grouped[unavailable["action_id"]]
    unavailable_rows.sort(key=lambda item: item["_system_from"])
    hidden = unavailable_rows[1]
    if hidden["external_outcome"] != "INDETERMINATE":
        fail("XTDB unavailable revision leaked external outcome")
    if parse_int(hidden["effect_count"]) is not None:
        fail("XTDB unavailable revision leaked hidden effect count")
    if hidden["liminal_continuity_posture"] != "REVALIDATE":
        fail("XTDB unavailable revision escaped REVALIDATE")

    return {
        "current_rows": 3,
        "history_rows": 7,
        "unavailable_sequence": [row["knowledge_state"] for row in unavailable_rows],
    }


def verify_neo4j_rows(
    envelope: dict[str, Any],
    rows: list[dict[str, str]],
) -> dict[str, Any]:
    if len(rows) != 4:
        fail(f"Neo4j explanation rows={len(rows)}, expected 4")

    by_case_revision: dict[tuple[str, int], dict[str, str]] = {}
    for row in rows:
        key = (row["case_id"], int(row["revision"]))
        if key in by_case_revision:
            fail(f"duplicate Neo4j explanation row: {key}")
        by_case_revision[key] = row

    for action in envelope["actions"]:
        for revision in action["revisions"]:
            key = (action["case_id"], revision["revision"])
            row = by_case_revision.get(key)
            if row is None:
                fail(f"missing Neo4j explanation row {key}")
            source = revision["source"]
            liminal = revision["liminal"]
            expected = {
                "availability": source["observation_availability"],
                "external_outcome": source["external_outcome"],
                "authority": liminal["authority"],
                "posture": liminal["continuity_posture"],
                "source_recovery_decision": source["recovery_decision"],
            }
            for field, value in expected.items():
                if row[field] != str(value):
                    fail(f"{key}: Neo4j {field} drift")

            observed_effects = int(row["observed_effects"])
            receiver_effect_count = int(row["receiver_effect_count"])
            if receiver_effect_count != action["receiver_truth"]["effect_count"]:
                fail(f"{key}: Neo4j receiver truth effect count drift")
            expected_observed = (
                1
                if source["observation_availability"] == "FULL"
                and source["effect_count"] == 1
                else 0
            )
            if observed_effects != expected_observed:
                fail(f"{key}: Neo4j observation/effect provenance drift")

    hidden = by_case_revision[("unavailable_readback", 1)]
    if int(hidden["receiver_effect_count"]) != 1 or int(hidden["observed_effects"]) != 0:
        fail("Neo4j unavailable phase failed hidden-effect provenance boundary")

    return {
        "rows": 4,
        "hidden_effect_boundary": "PASS",
    }


def verify(args: argparse.Namespace) -> None:
    fixture = load_json(args.fixture)
    validate_fixture(fixture)
    envelope = load_json(args.envelope)

    unsigned = copy.deepcopy(envelope)
    supplied_hash = unsigned.pop("envelope_sha256", None)
    if supplied_hash != sha256_json(unsigned):
        fail("canonical durability envelope SHA-256 mismatch")

    marker = args.liminal_marker.read_text(encoding="utf-8").strip()
    if marker != "PASS":
        fail("native LiminalDB durability mapping marker is not PASS")

    xtdb = verify_xtdb(
        envelope,
        read_csv(args.xtdb_current),
        read_csv(args.xtdb_history),
    )
    neo4j = verify_neo4j_rows(
        envelope,
        read_csv(args.neo4j_rows),
    )

    result = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "envelope_sha256": envelope["envelope_sha256"],
        "source_report_sha256": envelope["source"]["source_report_sha256"],
        "liminaldb_native_mapping": "PASS",
        "xtdb_system_time_history": xtdb,
        "neo4j_explanation_graph": neo4j,
        "cross_plane_invariant": (
            "Durable PENDING plus process crash never creates redispatch authority. "
            "Unavailable observation stays INDETERMINATE/REVALIDATE even when receiver truth "
            "contains an effect; only later FULL readback may finalize the existing action."
        ),
        "claim_ceiling": envelope["claim_ceiling"],
    }
    write_json(args.report, result)
    print(json.dumps(result, indent=2, sort_keys=True))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    sub = root.add_subparsers(dest="command", required=True)

    build_cmd = sub.add_parser("build")
    build_cmd.add_argument("--source-report", type=Path, required=True)
    build_cmd.add_argument("--fixture", type=Path, required=True)
    build_cmd.add_argument("--envelope", type=Path, required=True)
    build_cmd.add_argument("--xtdb-pending-sql", type=Path, required=True)
    build_cmd.add_argument("--xtdb-revision1-sql", type=Path, required=True)
    build_cmd.add_argument("--xtdb-revision2-sql", type=Path, required=True)
    build_cmd.add_argument("--neo4j-load-cypher", type=Path, required=True)
    build_cmd.add_argument("--neo4j-verify-cypher", type=Path, required=True)
    build_cmd.set_defaults(func=build)

    verify_cmd = sub.add_parser("verify")
    verify_cmd.add_argument("--fixture", type=Path, required=True)
    verify_cmd.add_argument("--envelope", type=Path, required=True)
    verify_cmd.add_argument("--liminal-marker", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-current", type=Path, required=True)
    verify_cmd.add_argument("--xtdb-history", type=Path, required=True)
    verify_cmd.add_argument("--neo4j-rows", type=Path, required=True)
    verify_cmd.add_argument("--report", type=Path, required=True)
    verify_cmd.set_defaults(func=verify)

    return root


def main() -> None:
    args = parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
