#!/usr/bin/env python3
"""Validate ProofPath SCIG verification receipts as LiminalDB AuditEvent artifacts.

This contract is deliberately artifact-only. It proves structural compatibility with
LiminalDB's current AuditEvent surface while keeping ProofPath provenance, semantic
identity, persistence, and execution authority as separate facts.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
from datetime import datetime
from pathlib import Path
from typing import Any

EVENT_SCHEMA = "liminaldb-proofpath-audit-event-v0.1"
SUMMARY_SCHEMA = "liminaldb-proofpath-import-check-v0.1"
EXPECTED_ACTOR = "proofpath-scig-native-verifier"
EXPECTED_ACTION = "proofpath.scig.verification.observed"
EXPECTED_PRODUCER_REPOSITORY = "safal207/ProofPath"
EXPECTED_CAPABILITY_ID = "proofpath.scig.v0.1"
EXPECTED_CAPABILITY_COMMIT = "685d50e256a5125a21f4c4584b326411caaa64ad"
EXPECTED_CONSUMER_REPOSITORY = "safal207/LiminalDB"
EXPECTED_CONTRACT_PATH = "sdk/ts/src/protocol-types.ts"
EXPECTED_EVENT_CONTRACT = "AuditEvent"
EXPECTED_NATIVE_VERIFIER = "proofpath-scig"
EXPECTED_VERIFICATION_CLASS = "native_recomputed"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
EVENT_ID = re.compile(r"^proofpath-[0-9a-f]{32}$")
ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT_FILE = ROOT / EXPECTED_CONTRACT_PATH
AUTHORITY_FIELDS = ("execution", "mutation", "persistence", "deployment", "merge")
PERSISTENCE_FALSE_FIELDS = ("durable_memory", "live_ingestion", "namespace_mutation")


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_json(value)).hexdigest()


def git_blob_sha(path: Path) -> str:
    if not path.is_file():
        raise ValueError(f"contract file does not exist: {path}")
    payload = path.read_bytes()
    return hashlib.sha1(f"blob {len(payload)}\0".encode("ascii") + payload).hexdigest()


def mapping(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{context} must be an object")
    return value


def text(obj: dict[str, Any], key: str, context: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{context}.{key} must be a non-empty string")
    return value.strip()


def validate_timestamp(value: str, context: str) -> None:
    candidate = value[:-1] + "+00:00" if value.endswith("Z") else value
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        raise ValueError(f"{context} must be ISO-8601") from exc
    if parsed.tzinfo is None:
        raise ValueError(f"{context} must include an explicit timezone")


def validate_event(value: Any, context: str, *, current_contract_blob_sha: str) -> dict[str, Any]:
    event = mapping(value, context)
    if EVENT_ID.fullmatch(text(event, "id", context)) is None:
        raise ValueError(f"{context}.id has invalid ProofPath event format")
    validate_timestamp(text(event, "ts", context), f"{context}.ts")
    if event.get("kind") != "audit":
        raise ValueError(f"{context}.kind must be audit")
    if event.get("actor") != EXPECTED_ACTOR:
        raise ValueError(f"{context}.actor must be {EXPECTED_ACTOR}")
    if event.get("action") != EXPECTED_ACTION:
        raise ValueError(f"{context}.action must be {EXPECTED_ACTION}")

    details = mapping(event.get("details"), f"{context}.details")
    if details.get("schema_version") != EVENT_SCHEMA:
        raise ValueError(f"{context}.details.schema_version is unsupported")

    logical_operation_id = text(details, "logical_operation_id", f"{context}.details")
    if event.get("correlationId") != logical_operation_id:
        raise ValueError(f"{context}.correlationId must equal details.logical_operation_id")

    source = mapping(details.get("source"), f"{context}.details.source")
    exact_source = {
        "repository": EXPECTED_PRODUCER_REPOSITORY,
        "capability_id": EXPECTED_CAPABILITY_ID,
        "capability_commit": EXPECTED_CAPABILITY_COMMIT,
        "native_result": "VALID",
        "native_verifier": EXPECTED_NATIVE_VERIFIER,
        "verification_class": EXPECTED_VERIFICATION_CLASS,
    }
    for key, expected in exact_source.items():
        if source.get(key) != expected:
            raise ValueError(f"{context}.details.source.{key} must equal {expected}")
    if HEX40.fullmatch(source["capability_commit"]) is None:
        raise ValueError(f"{context}.details.source.capability_commit must be a full SHA")
    text(source, "incident_id", f"{context}.details.source")
    for key in ("scig_sha256", "bridge_receipt_sha256"):
        if HEX64.fullmatch(text(source, key, f"{context}.details.source")) is None:
            raise ValueError(f"{context}.details.source.{key} must be SHA-256")

    evidence = mapping(details.get("evidence"), f"{context}.details.evidence")
    for key in ("bounded", "replayable", "source_receipt_bound"):
        if evidence.get(key) is not True:
            raise ValueError(f"{context}.details.evidence.{key} must be true")

    authority = mapping(details.get("authority"), f"{context}.details.authority")
    if authority.get("mode") != "evidence_only":
        raise ValueError(f"{context}.details.authority.mode must be evidence_only")
    for key in AUTHORITY_FIELDS:
        if authority.get(key) is not False:
            raise ValueError(f"{context}.details.authority.{key} must be false")

    persistence = mapping(details.get("persistence"), f"{context}.details.persistence")
    if persistence.get("write_mode") != "artifact_only":
        raise ValueError(f"{context}.details.persistence.write_mode must be artifact_only")
    for key in PERSISTENCE_FALSE_FIELDS:
        if persistence.get(key) is not False:
            raise ValueError(f"{context}.details.persistence.{key} must be false")

    adapter = mapping(details.get("adapter"), f"{context}.details.adapter")
    if adapter.get("repository") != EXPECTED_CONSUMER_REPOSITORY:
        raise ValueError(f"{context}.details.adapter.repository must equal {EXPECTED_CONSUMER_REPOSITORY}")
    if HEX40.fullmatch(text(adapter, "commit", f"{context}.details.adapter")) is None:
        raise ValueError(f"{context}.details.adapter.commit must be a full historical SHA")
    if adapter.get("contract_path") != EXPECTED_CONTRACT_PATH:
        raise ValueError(f"{context}.details.adapter.contract_path must equal {EXPECTED_CONTRACT_PATH}")
    if adapter.get("event_contract") != EXPECTED_EVENT_CONTRACT:
        raise ValueError(f"{context}.details.adapter.event_contract must equal {EXPECTED_EVENT_CONTRACT}")
    if adapter.get("write_mode") != "artifact_only":
        raise ValueError(f"{context}.details.adapter.write_mode must be artifact_only")
    declared_blob = text(adapter, "contract_blob_sha", f"{context}.details.adapter")
    if HEX40.fullmatch(declared_blob) is None or declared_blob != current_contract_blob_sha:
        raise ValueError(
            f"{context}.details.adapter.contract_blob_sha incompatible with current checkout: "
            f"declared={declared_blob} current={current_contract_blob_sha}"
        )

    recorded = text(details, "event_sha256", f"{context}.details")
    if HEX64.fullmatch(recorded) is None:
        raise ValueError(f"{context}.details.event_sha256 must be SHA-256")
    unhashed = copy.deepcopy(event)
    unhashed["details"].pop("event_sha256", None)
    if sha256_json(unhashed) != recorded:
        raise ValueError(f"{context}.details.event_sha256 mismatch")
    return event


def load_events(path: Path, *, contract_file: Path = DEFAULT_CONTRACT_FILE) -> list[dict[str, Any]]:
    if not path.is_file():
        raise ValueError(f"{path} does not exist or is not a file")
    current_blob = git_blob_sha(contract_file)
    events: list[dict[str, Any]] = []
    seen: set[str] = set()
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw.strip():
            continue
        try:
            decoded = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_number} is invalid JSON") from exc
        event = validate_event(decoded, f"{path}:{line_number}", current_contract_blob_sha=current_blob)
        if event["id"] in seen:
            raise ValueError(f"{path}:{line_number} duplicates event id {event['id']}")
        seen.add(event["id"])
        events.append(event)
    if not events:
        raise ValueError(f"{path} contains no ProofPath audit events")
    return events


def build_summary(events: list[dict[str, Any]], *, contract_file: Path = DEFAULT_CONTRACT_FILE, consumer_commit: str | None = None) -> dict[str, Any]:
    if consumer_commit is not None and HEX40.fullmatch(consumer_commit) is None:
        raise ValueError("consumer_commit must be a lowercase 40-character Git SHA")
    ordered = sorted(events, key=lambda event: (event["ts"], event["id"]))
    return {
        "schema_version": SUMMARY_SCHEMA,
        "mode": "dry_run",
        "write_performed": False,
        "event_count": len(ordered),
        "logical_operation_ids": sorted({event["details"]["logical_operation_id"] for event in ordered}),
        "event_ids": [event["id"] for event in ordered],
        "source": {
            "repository": EXPECTED_PRODUCER_REPOSITORY,
            "capability_id": EXPECTED_CAPABILITY_ID,
            "canonical_capability_commit": EXPECTED_CAPABILITY_COMMIT,
            "verification_class": EXPECTED_VERIFICATION_CLASS,
        },
        "compatibility": {
            "repository": EXPECTED_CONSUMER_REPOSITORY,
            "consumer_commit": consumer_commit,
            "contract_path": EXPECTED_CONTRACT_PATH,
            "event_contract": EXPECTED_EVENT_CONTRACT,
            "current_contract_blob_sha": git_blob_sha(contract_file),
            "historical_snapshot_is_semantic_key": False,
            "contract_blob_matches_current_checkout": True,
        },
        "authority": {
            "mode": "evidence_only",
            "execution_authorized": False,
            "mutation_authorized": False,
            "durable_memory_accepted": False,
            "live_ingestion_performed": False,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--contract-file", type=Path, default=DEFAULT_CONTRACT_FILE)
    parser.add_argument("--consumer-commit")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    summary = build_summary(
        load_events(args.events, contract_file=args.contract_file),
        contract_file=args.contract_file,
        consumer_commit=args.consumer_commit,
    )
    rendered = json.dumps(summary, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")


if __name__ == "__main__":
    main()
