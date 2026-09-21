#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import sqlite3
from pathlib import Path
from typing import Any

SYSTEM_CASE = "KNOWLEDGE-LAG-001"


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} SOURCE VERIFY FAIL: {message}")


def require(condition: bool, message: str) -> None:
    if not condition:
        fail(message)


def ro(path: Path) -> sqlite3.Connection:
    conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def integrity(path: Path) -> None:
    with ro(path) as conn:
        row = conn.execute("pragma integrity_check").fetchone()
    require(row is not None and row[0] == "ok", f"integrity_check failed: {path}")


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--control", type=Path, required=True)
    ap.add_argument("--receiver", type=Path, required=True)
    ap.add_argument("--observer", type=Path, required=True)
    ap.add_argument("--report", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    args = ap.parse_args()

    for path in (args.control, args.receiver, args.observer):
        require(path.exists(), f"missing DB: {path}")
        integrity(path)

    report = json.loads(args.report.read_text(encoding="utf-8"))
    require(report.get("system_case") == SYSTEM_CASE, "wrong source report system_case")
    require(report.get("status") == "OBSERVED", "source report is not OBSERVED")

    action = report["action"]
    action_id = action["action_id"]
    target = action["target"]
    payload_digest = action["payload_digest"]

    with ro(args.control) as conn:
        admissions = conn.execute(
            "select * from admissions where action_id=?", (action_id,)
        ).fetchall()
        claims = conn.execute(
            "select * from claims where action_id=? order by seq", (action_id,)
        ).fetchall()

    require(len(admissions) == 1, "expected one admission")
    require(len(claims) == 1, "expected one claim")
    admission = admissions[0]
    claim = claims[0]

    require(admission["target"] == target, "admission target drift")
    require(admission["payload_digest"] == payload_digest, "admission payload digest drift")
    require(int(admission["authority_consumed"]) == 1, "dispatch authority not consumed")
    require(claim["claim"] == "SUCCESS", "client claim is not SUCCESS")

    with ro(args.receiver) as conn:
        attempts = conn.execute(
            "select * from attempts where action_id=? order by seq", (action_id,)
        ).fetchall()
        effects = conn.execute(
            "select * from effects where action_id=? order by committed_at_ns", (action_id,)
        ).fetchall()

    require(len(attempts) == 1, "expected one provider attempt")
    require(len(effects) == 1, "expected one receiver effect")
    attempt = attempts[0]
    effect = effects[0]

    require(attempt["target"] == target, "attempt target drift")
    require(attempt["payload_digest"] == payload_digest, "attempt payload digest drift")
    require(effect["target"] == target, "effect target drift")
    require(effect["payload_digest"] == payload_digest, "effect payload digest drift")
    require(effect["result"] == "effect-ok", "effect result drift")

    with ro(args.observer) as conn:
        observations = conn.execute(
            "select * from observations where action_id=? order by revision", (action_id,)
        ).fetchall()

    require(len(observations) == 2, "expected two readback observations")
    unavailable, full = observations

    require(int(unavailable["revision"]) == 1, "UNAVAILABLE must be revision 1")
    require(unavailable["availability"] == "UNAVAILABLE", "revision 1 availability drift")
    require(unavailable["external_outcome"] == "INDETERMINATE", "revision 1 outcome drift")
    require(int(unavailable["externally_verified"]) == 0, "UNAVAILABLE marked verified")
    require(unavailable["effect_count"] is None, "UNAVAILABLE leaked effect count")
    require(unavailable["observed_effect_id"] is None, "UNAVAILABLE leaked effect identity")
    require(
        unavailable["observed_effect_committed_at_ns"] is None,
        "UNAVAILABLE leaked effect valid time",
    )

    require(int(full["revision"]) == 2, "FULL must be revision 2")
    require(full["availability"] == "FULL", "revision 2 availability drift")
    require(full["external_outcome"] == "ONE_EFFECT_MATCHING", "revision 2 outcome drift")
    require(int(full["externally_verified"]) == 1, "FULL readback is not verified")
    require(int(full["effect_count"]) == 1, "FULL effect count drift")
    require(full["observed_effect_id"] == effect["effect_id"], "FULL effect id mismatch")
    require(
        int(full["observed_effect_committed_at_ns"]) == int(effect["committed_at_ns"]),
        "FULL readback changed effect valid time",
    )
    require(
        full["observed_effect_committed_at_utc"] == effect["committed_at_utc"],
        "FULL readback changed effect UTC time",
    )

    timeline = [
        int(admission["admitted_at_ns"]),
        int(admission["dispatch_started_at_ns"]),
        int(effect["committed_at_ns"]),
        int(claim["claimed_at_ns"]),
        int(unavailable["observed_at_ns"]),
        int(full["observed_at_ns"]),
    ]
    require(
        all(left < right for left, right in zip(timeline, timeline[1:], strict=True)),
        "expected T0 < T0D < T1 < T2 < T3 < T4",
    )

    report_timeline = report["timeline"]
    expected_report_times = [
        int(report_timeline["T0_admitted"]["ns"]),
        int(report_timeline["T0D_dispatch_started"]["ns"]),
        int(report_timeline["T1_effect_committed"]["ns"]),
        int(report_timeline["T2_claim_success"]["ns"]),
        int(report_timeline["T3_readback_unavailable"]["ns"]),
        int(report_timeline["T4_readback_full"]["ns"]),
    ]
    require(timeline == expected_report_times, "report timeline differs from source DBs")

    require(
        report["receiver"]["attempt_count"] == 1
        and report["receiver"]["effect_count"] == 1,
        "report receiver cardinality drift",
    )
    require(report["claim"]["value"] == "SUCCESS", "report claim drift")
    require(report["observations"][0]["effect_count"] is None, "report T3 leaked effect count")
    require(
        report["observations"][0]["observed_effect_id"] is None,
        "report T3 leaked effect id",
    )
    require(
        report["observations"][1]["observed_effect_id"] == effect["effect_id"],
        "report T4 effect id mismatch",
    )

    hashes = {
        "control": sha256_file(args.control),
        "receiver": sha256_file(args.receiver),
        "observer": sha256_file(args.observer),
    }
    require(hashes == report["source_db_sha256"], "source DB SHA-256 drift")

    result: dict[str, Any] = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "verified": [
            "action_id/payload/target admitted before dispatch",
            "exactly one provider attempt and one receiver effect",
            "receiver effect valid-time T1 precedes SUCCESS claim T2",
            "T3 UNAVAILABLE contains no effect id/count/valid-time despite receiver effect existence",
            "T4 FULL readback binds the exact T1 receiver effect",
            "T0 < T0D < T1 < T2 < T3 < T4",
            "source report matches independently reopened SQLite authority stores",
        ],
        "source_db_sha256": hashes,
        "effect_valid_time_utc": effect["committed_at_utc"],
        "unavailable_observed_at_utc": unavailable["observed_at_utc"],
        "full_observed_at_utc": full["observed_at_utc"],
        "claim_ceiling":
            "Same-host independent SQLite authority fixture; no distributed clock or production-provider claim.",
    }

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
