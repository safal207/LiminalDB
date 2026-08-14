#!/usr/bin/env python3
"""Validate Pythia/Lotus verified-negative AuditEvent JSONL without mutation."""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import re
from datetime import datetime
from pathlib import Path
from typing import Any

EVENT_SCHEMA = "liminaldb-pythia-negative-memory-event-v0.1"
EXPECTED_ACTOR = "pythia-lotus"
EXPECTED_ACTION = "lotus.verified_negative.observed"
EXPECTED_PYTHIA_REPOSITORY = "safal207/pythiaLabs"
EXPECTED_PYTHIA_COMMIT = "92b4ade7c2057f6d5cf542dca1bf474360cd74c1"
GRANTS = (
    "ownership",
    "approval",
    "execution",
    "delivery",
    "external_submission",
    "deployment",
    "merge",
    "durable_memory_write",
)
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
SHA_REF = re.compile(r"^sha256:[0-9a-f]{64}$")
EVENT_ID = re.compile(r"^pythia-neg-[0-9a-f]{32}$")


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_json(value)).hexdigest()


def mapping(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{context} must be an object")
    return value


def text(obj: dict[str, Any], key: str, context: str) -> str:
    value = obj.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{context}.{key} must be a non-empty string")
    return value


def validate_timestamp(value: str, context: str) -> None:
    candidate = value[:-1] + "+00:00" if value.endswith("Z") else value
    try:
        parsed = datetime.fromisoformat(candidate)
    except ValueError as exc:
        raise ValueError(f"{context} must be ISO-8601") from exc
    if parsed.tzinfo is None:
        raise ValueError(f"{context} must include an explicit timezone")


def validate_event(value: Any, context: str) -> dict[str, Any]:
    event = mapping(value, context)
    if EVENT_ID.fullmatch(text(event, "id", context)) is None:
        raise ValueError(f"{context}.id has invalid format")
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

    source = mapping(details.get("source"), f"{context}.details.source")
    for key in ("repository", "branch", "packet_id"):
        text(source, key, f"{context}.details.source")
    if HEX40.fullmatch(text(source, "commit", f"{context}.details.source")) is None:
        raise ValueError(f"{context}.details.source.commit must be a full SHA")
    if HEX64.fullmatch(text(source, "packet_sha256", f"{context}.details.source")) is None:
        raise ValueError(f"{context}.details.source.packet_sha256 must be SHA-256")
    for key in ("authorization_ref", "observation_ref", "observation_result_ref"):
        if SHA_REF.fullmatch(text(source, key, f"{context}.details.source")) is None:
            raise ValueError(f"{context}.details.source.{key} must be sha256:<64hex>")

    judgment = mapping(details.get("judgment"), f"{context}.details.judgment")
    exact_judgment = {
        "schema_version": "pythia-lotus-ltp-judgment-v0.1",
        "decision_status": "CONFIRMED",
        "pythia_verdict": "ALLOW",
        "result_class": "VERIFIED_NEGATIVE_OBSERVATION",
        "memory_kind": "VERIFIED_NEGATIVE_OBSERVATION",
        "cause_status": "UNCONFIRMED",
        "confidence": "OBSERVED_ONCE",
        "recurrence": "SINGLE_TRANSITION",
        "durable_memory": False,
    }
    for key, expected in exact_judgment.items():
        if judgment.get(key) != expected:
            raise ValueError(f"{context}.details.judgment.{key} must equal {expected}")
    text(judgment, "canonical_id", f"{context}.details.judgment")
    if HEX64.fullmatch(text(judgment, "sha256", f"{context}.details.judgment")) is None:
        raise ValueError(f"{context}.details.judgment.sha256 must be SHA-256")

    evidence = mapping(details.get("evidence"), f"{context}.details.evidence")
    exact_evidence = {
        "bounded": True,
        "replayable": True,
        "ltp_verification_level": "FULL_LIFECYCLE_JOINED",
        "response_integrity": "VERIFIED",
        "fabricated_claim_control": "CONTRADICTED",
    }
    for key, expected in exact_evidence.items():
        if evidence.get(key) != expected:
            raise ValueError(f"{context}.details.evidence.{key} must equal {expected}")

    authority = mapping(details.get("authority"), f"{context}.details.authority")
    if authority.get("mode") != "audit_only":
        raise ValueError(f"{context}.details.authority.mode must be audit_only")
    for grant in GRANTS:
        if authority.get(grant) is not False:
            raise ValueError(f"{context}.details.authority.{grant} must be false")

    adapter = mapping(details.get("adapter"), f"{context}.details.adapter")
    exact_adapter = {
        "repository": EXPECTED_PYTHIA_REPOSITORY,
        "commit": EXPECTED_PYTHIA_COMMIT,
        "event_contract": "AuditEvent-extension",
        "write_mode": "artifact_only",
    }
    for key, expected in exact_adapter.items():
        if adapter.get(key) != expected:
            raise ValueError(f"{context}.details.adapter.{key} must equal {expected}")

    recorded = text(details, "event_sha256", f"{context}.details")
    if HEX64.fullmatch(recorded) is None:
        raise ValueError(f"{context}.details.event_sha256 must be SHA-256")
    unhashed = copy.deepcopy(event)
    unhashed["details"].pop("event_sha256", None)
    if sha256_json(unhashed) != recorded:
        raise ValueError(f"{context}.details.event_sha256 mismatch")
    return event


def load_events(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        raise ValueError(f"{path} does not exist")
    events: list[dict[str, Any]] = []
    seen: set[str] = set()
    for line_number, raw in enumerate(path.read_text().splitlines(), start=1):
        if not raw.strip():
            continue
        try:
            value = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_number} is invalid JSON") from exc
        event = validate_event(value, f"{path}:{line_number}")
        if event["id"] in seen:
            raise ValueError(f"{path}:{line_number} duplicates event id {event['id']}")
        seen.add(event["id"])
        events.append(event)
    if not events:
        raise ValueError(f"{path} contains no events")
    return events


def build_summary(events: list[dict[str, Any]]) -> dict[str, Any]:
    ordered = sorted(events, key=lambda event: (event["ts"], event["id"]))
    return {
        "schema_version": "liminaldb-pythia-negative-import-check-v0.1",
        "mode": "dry_run",
        "write_performed": False,
        "event_count": len(ordered),
        "verified_negative_count": len(ordered),
        "canonical_memory_count": len({event["details"]["judgment"]["canonical_id"] for event in ordered}),
        "event_ids": [event["id"] for event in ordered],
        "transition_ids": [event["details"]["source"]["packet_id"] for event in ordered],
        "authority": {
            "mode": "audit_only",
            "durable_memory_accepted": False,
            "live_ingestion_performed": False,
            "external_submission_authorized": False,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--events", type=Path, required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    summary = build_summary(load_events(args.events))
    rendered = json.dumps(summary, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered)
    else:
        print(rendered, end="")


if __name__ == "__main__":
    main()
