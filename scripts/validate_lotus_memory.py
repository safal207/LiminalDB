#!/usr/bin/env python3
"""Validate LiminalQA Lotus memory AuditEvent JSONL without mutating LiminalDB.

Compatibility is content-addressed: a historical producer may name the exact
LiminalDB commit it targeted, but acceptance against the current checkout is
based on the actual Git-blob identity of the declared contract file. This keeps
historical provenance separate from current semantic compatibility.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from datetime import datetime
from pathlib import Path
from typing import Any

EVENT_SCHEMA = "liminaldb-lotus-memory-event-v0.1"
EXPECTED_ACTOR = "liminalqa-lotus"
EXPECTED_ACTION = "lotus.finding.observed"
EXPECTED_REPOSITORY = "safal207/LiminalDB"
EXPECTED_CONTRACT_PATH = "sdk/ts/src/protocol-types.ts"
EXPECTED_EVENT_CONTRACT = "AuditEvent"
AUTHORITY_GRANTS = ("ownership", "approval", "execution", "delivery", "deployment", "merge")
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
EVENT_ID = re.compile(r"^lotus-[0-9a-f]{32}$")
ROOT = Path(__file__).resolve().parents[1]
DEFAULT_CONTRACT_FILE = ROOT / EXPECTED_CONTRACT_PATH


def canonical_json(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_json(value)).hexdigest()


def git_blob_sha(path: Path) -> str:
    """Return the Git blob SHA-1 for the exact checked-out file bytes."""

    if not path.is_file():
        raise ValueError(f"contract file does not exist: {path}")
    payload = path.read_bytes()
    header = f"blob {len(payload)}\0".encode("ascii")
    return hashlib.sha1(header + payload).hexdigest()


def require_mapping(value: Any, context: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{context} must be an object")
    return value


def require_string(obj: dict[str, Any], key: str, context: str) -> str:
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


def validate_authority(value: Any, context: str) -> dict[str, Any]:
    authority = require_mapping(value, context)
    if authority.get("mode") != "audit_only":
        raise ValueError(f"{context}.mode must be audit_only")
    for grant in AUTHORITY_GRANTS:
        if authority.get(grant) is not False:
            raise ValueError(f"{context}.{grant} must be false")
    return authority


def validate_adapter(
    value: Any,
    context: str,
    *,
    current_contract_blob_sha: str,
) -> dict[str, Any]:
    adapter = require_mapping(value, context)

    if adapter.get("repository") != EXPECTED_REPOSITORY:
        raise ValueError(f"{context}.repository must equal {EXPECTED_REPOSITORY}")

    historical_commit = require_string(adapter, "commit", context)
    if HEX40.fullmatch(historical_commit) is None:
        raise ValueError(f"{context}.commit must be a full historical commit SHA")

    if adapter.get("contract_path") != EXPECTED_CONTRACT_PATH:
        raise ValueError(f"{context}.contract_path must equal {EXPECTED_CONTRACT_PATH}")
    if adapter.get("event_contract") != EXPECTED_EVENT_CONTRACT:
        raise ValueError(f"{context}.event_contract must equal {EXPECTED_EVENT_CONTRACT}")
    if adapter.get("write_mode") != "artifact_only":
        raise ValueError(f"{context}.write_mode must equal artifact_only")

    declared_blob = require_string(adapter, "contract_blob_sha", context)
    if HEX40.fullmatch(declared_blob) is None:
        raise ValueError(f"{context}.contract_blob_sha must be a Git blob SHA")
    if declared_blob != current_contract_blob_sha:
        raise ValueError(
            f"{context}.contract_blob_sha is incompatible with current checked-out contract: "
            f"declared={declared_blob} current={current_contract_blob_sha}"
        )

    return adapter


def validate_event(
    event: Any,
    context: str,
    *,
    current_contract_blob_sha: str,
) -> dict[str, Any]:
    event = require_mapping(event, context)

    event_id = require_string(event, "id", context)
    if EVENT_ID.fullmatch(event_id) is None:
        raise ValueError(f"{context}.id has invalid Lotus event format")

    timestamp = require_string(event, "ts", context)
    validate_timestamp(timestamp, f"{context}.ts")

    if event.get("kind") != "audit":
        raise ValueError(f"{context}.kind must be audit")
    if event.get("actor") != EXPECTED_ACTOR:
        raise ValueError(f"{context}.actor must be {EXPECTED_ACTOR}")
    if event.get("action") != EXPECTED_ACTION:
        raise ValueError(f"{context}.action must be {EXPECTED_ACTION}")

    details = require_mapping(event.get("details"), f"{context}.details")
    if details.get("schema_version") != EVENT_SCHEMA:
        raise ValueError(f"{context}.details.schema_version is unsupported")

    source = require_mapping(details.get("source"), f"{context}.details.source")
    if HEX40.fullmatch(require_string(source, "commit", f"{context}.details.source")) is None:
        raise ValueError(f"{context}.details.source.commit must be a full commit SHA")
    for key in ("packet_sha256", "finding_sha256"):
        if HEX64.fullmatch(require_string(source, key, f"{context}.details.source")) is None:
            raise ValueError(f"{context}.details.source.{key} must be SHA-256")
    for key in ("repository", "branch", "packet_id"):
        require_string(source, key, f"{context}.details.source")

    finding = require_mapping(details.get("finding"), f"{context}.details.finding")
    for key in ("finding_id", "canonical_id", "decision_status", "pythia_verdict"):
        require_string(finding, key, f"{context}.details.finding")
    if finding["decision_status"] not in {"CONFIRMED", "BLOCKED", "NEEDS_EVIDENCE"}:
        raise ValueError(f"{context}.details.finding.decision_status is unsupported")
    if finding["pythia_verdict"] not in {"ALLOW", "BLOCK", "ESCALATE"}:
        raise ValueError(f"{context}.details.finding.pythia_verdict is unsupported")
    if finding.get("durable_memory") is not False:
        raise ValueError(
            f"{context}.details.finding.durable_memory must remain false for artifact-only import"
        )

    evidence = require_mapping(details.get("evidence"), f"{context}.details.evidence")
    if evidence.get("bounded") is not True or evidence.get("replayable") is not True:
        raise ValueError(f"{context}.details.evidence must be bounded and replayable")

    validate_authority(details.get("authority"), f"{context}.details.authority")
    validate_adapter(
        details.get("adapter"),
        f"{context}.details.adapter",
        current_contract_blob_sha=current_contract_blob_sha,
    )

    recorded_hash = require_string(details, "event_sha256", f"{context}.details")
    if HEX64.fullmatch(recorded_hash) is None:
        raise ValueError(f"{context}.details.event_sha256 must be SHA-256")
    unhashed = json.loads(json.dumps(event))
    unhashed["details"].pop("event_sha256", None)
    actual_hash = sha256_json(unhashed)
    if actual_hash != recorded_hash:
        raise ValueError(f"{context}.details.event_sha256 mismatch")

    return event


def load_events(
    path: Path,
    *,
    contract_file: Path = DEFAULT_CONTRACT_FILE,
) -> list[dict[str, Any]]:
    if not path.is_file():
        raise ValueError(f"{path} does not exist or is not a file")

    current_contract_blob_sha = git_blob_sha(contract_file)
    events: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for line_number, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw.strip():
            continue
        try:
            decoded = json.loads(raw)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{path}:{line_number} is not valid JSON") from exc
        event = validate_event(
            decoded,
            f"{path}:{line_number}",
            current_contract_blob_sha=current_contract_blob_sha,
        )
        if event["id"] in seen_ids:
            raise ValueError(f"{path}:{line_number} duplicates event id {event['id']}")
        seen_ids.add(event["id"])
        events.append(event)

    if not events:
        raise ValueError(f"{path} contains no Lotus memory events")
    return events


def build_summary(
    events: list[dict[str, Any]],
    *,
    contract_file: Path = DEFAULT_CONTRACT_FILE,
    consumer_commit: str | None = None,
) -> dict[str, Any]:
    if consumer_commit is not None and HEX40.fullmatch(consumer_commit) is None:
        raise ValueError("consumer_commit must be a lowercase 40-character Git SHA")

    ordered = sorted(events, key=lambda event: (event["ts"], event["id"]))
    decisions = Counter(
        event["details"]["finding"]["decision_status"] for event in ordered
    )
    verdicts = Counter(
        event["details"]["finding"]["pythia_verdict"] for event in ordered
    )
    adapter_commits = sorted(
        {event["details"]["adapter"]["commit"] for event in ordered}
    )
    current_blob = git_blob_sha(contract_file)

    return {
        "schema_version": "liminaldb-lotus-import-check-v0.2",
        "mode": "dry_run",
        "write_performed": False,
        "event_count": len(ordered),
        "canonical_finding_count": len(
            {event["details"]["finding"]["canonical_id"] for event in ordered}
        ),
        "first_timestamp": ordered[0]["ts"],
        "last_timestamp": ordered[-1]["ts"],
        "decision_statuses": dict(sorted(decisions.items())),
        "pythia_verdicts": dict(sorted(verdicts.items())),
        "event_ids": [event["id"] for event in ordered],
        "compatibility": {
            "repository": EXPECTED_REPOSITORY,
            "contract_path": EXPECTED_CONTRACT_PATH,
            "event_contract": EXPECTED_EVENT_CONTRACT,
            "current_contract_blob_sha": current_blob,
            "declared_adapter_commits": adapter_commits,
            "consumer_commit": consumer_commit,
            "historical_snapshot_is_semantic_key": False,
            "contract_blob_matches_current_checkout": True,
        },
        "authority": {
            "mode": "audit_only",
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

    events = load_events(args.events, contract_file=args.contract_file)
    summary = build_summary(
        events,
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
