#!/usr/bin/env python3
"""KNOWLEDGE-LAG-001: source authorities, XTDB phases, envelope, Neo4j and verification."""

from __future__ import annotations

import argparse
import copy
import csv
import hashlib
import json
import sqlite3
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

SYSTEM_CASE = "KNOWLEDGE-LAG-001"
ENVELOPE_SCHEMA = "liminaldb.knowledge-lag-envelope.v0.1"
XTDB_TABLE = "knowledge_lag_evidence"
TARGET = "receiver:knowledge-lag"
REQUEST = {"prompt": "retroactive-readback", "model": "bounded-local-fixture"}
ACTION_ID = "act_" + hashlib.sha256(f"{SYSTEM_CASE}|{TARGET}".encode()).hexdigest()[:32]
EFFECT_ID = "eff_" + hashlib.sha256(f"{ACTION_ID}|effect:1".encode()).hexdigest()[:32]


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def sha256_json(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


PAYLOAD_DIGEST = sha256_json(
    {"target": TARGET, "action": "provider.generate", "request": REQUEST}
)


def fail(message: str) -> None:
    raise SystemExit(f"{SYSTEM_CASE} FAIL: {message}")


def connect(path: Path, *, readonly: bool = False) -> sqlite3.Connection:
    if readonly:
        conn = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(path)
        conn.execute("pragma journal_mode=WAL")
        conn.execute("pragma synchronous=FULL")
        conn.execute("pragma foreign_keys=ON")
    conn.row_factory = sqlite3.Row
    return conn


def init_control(path: Path) -> None:
    with connect(path) as conn:
        conn.executescript(
            """
            create table if not exists admissions (
                action_id text primary key,
                target text not null,
                payload_digest text not null,
                request_json text not null,
                admitted_at_ns integer not null,
                admitted_at_utc text not null,
                dispatch_started_at_ns integer,
                dispatch_started_at_utc text,
                authority_consumed integer not null default 0 check (authority_consumed in (0,1))
            );
            create table if not exists claims (
                seq integer primary key autoincrement,
                action_id text not null,
                claim text not null,
                claimed_at_ns integer not null,
                claimed_at_utc text not null
            );
            """
        )
        conn.commit()


def init_receiver(path: Path) -> None:
    with connect(path) as conn:
        conn.executescript(
            """
            create table if not exists attempts (
                seq integer primary key autoincrement,
                action_id text not null,
                target text not null,
                payload_digest text not null,
                started_at_ns integer not null,
                started_at_utc text not null
            );
            create table if not exists effects (
                effect_id text primary key,
                action_id text not null,
                target text not null,
                payload_digest text not null,
                result text not null,
                committed_at_ns integer not null,
                committed_at_utc text not null
            );
            """
        )
        conn.commit()


def init_observer(path: Path) -> None:
    with connect(path) as conn:
        conn.executescript(
            """
            create table if not exists observations (
                revision integer primary key,
                action_id text not null,
                availability text not null,
                external_outcome text not null,
                externally_verified integer not null check (externally_verified in (0,1)),
                effect_count integer,
                observed_effect_id text,
                observed_effect_committed_at_ns integer,
                observed_effect_committed_at_utc text,
                observed_at_ns integer not null,
                observed_at_utc text not null
            );
            """
        )
        conn.commit()


def init_all(control: Path, receiver: Path, observer: Path) -> None:
    init_control(control)
    init_receiver(receiver)
    init_observer(observer)


def utc_from_ns(value: int) -> str:
    dt = datetime.fromtimestamp(value / 1_000_000_000, tz=timezone.utc)
    return dt.isoformat(timespec="microseconds")


def max_time_ns(control: Path, receiver: Path, observer: Path) -> int:
    candidates = [0]
    if control.exists():
        with connect(control, readonly=True) as conn:
            for query in (
                "select max(admitted_at_ns) from admissions",
                "select max(dispatch_started_at_ns) from admissions",
                "select max(claimed_at_ns) from claims",
            ):
                row = conn.execute(query).fetchone()
                if row and row[0] is not None:
                    candidates.append(int(row[0]))
    if receiver.exists():
        with connect(receiver, readonly=True) as conn:
            for query in (
                "select max(started_at_ns) from attempts",
                "select max(committed_at_ns) from effects",
            ):
                row = conn.execute(query).fetchone()
                if row and row[0] is not None:
                    candidates.append(int(row[0]))
    if observer.exists():
        with connect(observer, readonly=True) as conn:
            row = conn.execute("select max(observed_at_ns) from observations").fetchone()
            if row and row[0] is not None:
                candidates.append(int(row[0]))
    return max(candidates)


def next_source_time(control: Path, receiver: Path, observer: Path) -> tuple[int, str]:
    previous = max_time_ns(control, receiver, observer)
    # Keep source events distinct at microsecond SQL precision even on a coarse clock.
    now = time.time_ns()
    value = max(now, previous + 1_000_000)
    return value, utc_from_ns(value)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def phase_admit(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_all(control, receiver, observer)
    with connect(control) as conn:
        count = conn.execute("select count(*) from admissions").fetchone()[0]
        if count != 0:
            fail("admission phase requires empty admission ledger")
        ns, utc = next_source_time(control, receiver, observer)
        conn.execute(
            """
            insert into admissions(
                action_id, target, payload_digest, request_json,
                admitted_at_ns, admitted_at_utc
            ) values (?, ?, ?, ?, ?, ?)
            """,
            (ACTION_ID, TARGET, PAYLOAD_DIGEST, json.dumps(REQUEST, sort_keys=True), ns, utc),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(json.dumps({"phase": "T0_ADMISSION", "action_id": ACTION_ID, "at_ns": ns, "at_utc": utc}))


def phase_dispatch(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_all(control, receiver, observer)
    with connect(control, readonly=True) as conn:
        row = conn.execute(
            "select target, payload_digest, dispatch_started_at_ns from admissions where action_id=?",
            (ACTION_ID,),
        ).fetchone()
    if row is None or row["target"] != TARGET or row["payload_digest"] != PAYLOAD_DIGEST:
        fail("dispatch cannot find matching admission")
    if row["dispatch_started_at_ns"] is not None:
        fail("dispatch already started")

    ns, utc = next_source_time(control, receiver, observer)
    with connect(control) as conn:
        updated = conn.execute(
            """
            update admissions
            set dispatch_started_at_ns=?, dispatch_started_at_utc=?, authority_consumed=1
            where action_id=? and dispatch_started_at_ns is null
            """,
            (ns, utc, ACTION_ID),
        ).rowcount
        if updated != 1:
            fail("dispatch admission update failed")
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")

    with connect(receiver) as conn:
        conn.execute(
            """
            insert into attempts(action_id, target, payload_digest, started_at_ns, started_at_utc)
            values (?, ?, ?, ?, ?)
            """,
            (ACTION_ID, TARGET, PAYLOAD_DIGEST, ns, utc),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(json.dumps({"phase": "T0D_DISPATCH", "action_id": ACTION_ID, "at_ns": ns, "at_utc": utc}))


def phase_effect(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_all(control, receiver, observer)
    with connect(receiver, readonly=True) as conn:
        attempts = conn.execute(
            "select count(*) from attempts where action_id=?", (ACTION_ID,)
        ).fetchone()[0]
        effects = conn.execute(
            "select count(*) from effects where action_id=?", (ACTION_ID,)
        ).fetchone()[0]
    if attempts != 1 or effects != 0:
        fail("effect phase requires exactly one attempt and zero effects")

    ns, utc = next_source_time(control, receiver, observer)
    with connect(receiver) as conn:
        conn.execute(
            """
            insert into effects(
                effect_id, action_id, target, payload_digest, result,
                committed_at_ns, committed_at_utc
            ) values (?, ?, ?, ?, 'effect-ok', ?, ?)
            """,
            (EFFECT_ID, ACTION_ID, TARGET, PAYLOAD_DIGEST, ns, utc),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(
        json.dumps(
            {
                "phase": "T1_EFFECT_COMMITTED",
                "action_id": ACTION_ID,
                "effect_id": EFFECT_ID,
                "at_ns": ns,
                "at_utc": utc,
            }
        )
    )


def phase_claim(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_all(control, receiver, observer)
    with connect(control, readonly=True) as conn:
        admission = conn.execute(
            "select authority_consumed from admissions where action_id=?", (ACTION_ID,)
        ).fetchone()
        claims = conn.execute(
            "select count(*) from claims where action_id=?", (ACTION_ID,)
        ).fetchone()[0]
    if admission is None or int(admission["authority_consumed"]) != 1:
        fail("claim requires consumed dispatch authority")
    if claims != 0:
        fail("claim already recorded")

    ns, utc = next_source_time(control, receiver, observer)
    with connect(control) as conn:
        conn.execute(
            """
            insert into claims(action_id, claim, claimed_at_ns, claimed_at_utc)
            values (?, 'SUCCESS', ?, ?)
            """,
            (ACTION_ID, ns, utc),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(json.dumps({"phase": "T2_CLAIM_SUCCESS", "action_id": ACTION_ID, "at_ns": ns, "at_utc": utc}))


def phase_unavailable(args: argparse.Namespace) -> None:
    # Intentionally does not open the receiver DB. This phase cannot inspect hidden effect truth.
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_control(control)
    init_observer(observer)
    with connect(control, readonly=True) as conn:
        claim = conn.execute(
            "select claim from claims where action_id=? order by seq desc limit 1",
            (ACTION_ID,),
        ).fetchone()
    if claim is None or claim["claim"] != "SUCCESS":
        fail("UNAVAILABLE observation requires prior SUCCESS claim")
    with connect(observer, readonly=True) as conn:
        count = conn.execute("select count(*) from observations").fetchone()[0]
    if count != 0:
        fail("UNAVAILABLE observation must be first")

    ns, utc = next_source_time(control, receiver, observer)
    with connect(observer) as conn:
        conn.execute(
            """
            insert into observations(
                revision, action_id, availability, external_outcome,
                externally_verified, effect_count, observed_effect_id,
                observed_effect_committed_at_ns, observed_effect_committed_at_utc,
                observed_at_ns, observed_at_utc
            ) values (
                1, ?, 'UNAVAILABLE', 'INDETERMINATE',
                0, NULL, NULL, NULL, NULL, ?, ?
            )
            """,
            (ACTION_ID, ns, utc),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(json.dumps({"phase": "T3_READBACK_UNAVAILABLE", "action_id": ACTION_ID, "at_ns": ns, "at_utc": utc}))


def phase_full(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    init_all(control, receiver, observer)
    with connect(observer, readonly=True) as conn:
        first = conn.execute(
            "select availability, external_outcome from observations where revision=1"
        ).fetchone()
        count = conn.execute("select count(*) from observations").fetchone()[0]
    if first is None or first["availability"] != "UNAVAILABLE" or first["external_outcome"] != "INDETERMINATE":
        fail("FULL readback requires prior UNAVAILABLE observation")
    if count != 1:
        fail("FULL readback must be second observation")

    with connect(receiver, readonly=True) as conn:
        effects = conn.execute(
            """
            select effect_id, target, payload_digest, result, committed_at_ns, committed_at_utc
            from effects where action_id=? order by committed_at_ns
            """,
            (ACTION_ID,),
        ).fetchall()
    if len(effects) != 1:
        fail(f"FULL readback expected exactly one effect, got {len(effects)}")
    effect = effects[0]
    if (
        effect["effect_id"] != EFFECT_ID
        or effect["target"] != TARGET
        or effect["payload_digest"] != PAYLOAD_DIGEST
        or effect["result"] != "effect-ok"
    ):
        fail("FULL readback effect identity mismatch")

    ns, utc = next_source_time(control, receiver, observer)
    with connect(observer) as conn:
        conn.execute(
            """
            insert into observations(
                revision, action_id, availability, external_outcome,
                externally_verified, effect_count, observed_effect_id,
                observed_effect_committed_at_ns, observed_effect_committed_at_utc,
                observed_at_ns, observed_at_utc
            ) values (
                2, ?, 'FULL', 'ONE_EFFECT_MATCHING',
                1, 1, ?, ?, ?, ?, ?
            )
            """,
            (
                ACTION_ID,
                EFFECT_ID,
                int(effect["committed_at_ns"]),
                str(effect["committed_at_utc"]),
                ns,
                utc,
            ),
        )
        conn.commit()
        conn.execute("pragma wal_checkpoint(FULL)")
    print(
        json.dumps(
            {
                "phase": "T4_READBACK_FULL",
                "action_id": ACTION_ID,
                "effect_id": EFFECT_ID,
                "effect_committed_at_utc": effect["committed_at_utc"],
                "at_ns": ns,
                "at_utc": utc,
            }
        )
    )


def load_state(control: Path, receiver: Path, observer: Path) -> dict[str, Any]:
    with connect(control, readonly=True) as conn:
        admission = conn.execute(
            "select * from admissions where action_id=?", (ACTION_ID,)
        ).fetchone()
        claim = conn.execute(
            "select * from claims where action_id=? order by seq desc limit 1", (ACTION_ID,)
        ).fetchone()
    with connect(receiver, readonly=True) as conn:
        attempts = conn.execute(
            "select * from attempts where action_id=? order by seq", (ACTION_ID,)
        ).fetchall()
        effects = conn.execute(
            "select * from effects where action_id=? order by committed_at_ns", (ACTION_ID,)
        ).fetchall()
    with connect(observer, readonly=True) as conn:
        observations = conn.execute(
            "select * from observations where action_id=? order by revision", (ACTION_ID,)
        ).fetchall()

    return {
        "admission": None if admission is None else dict(admission),
        "claim": None if claim is None else dict(claim),
        "attempts": [dict(row) for row in attempts],
        "effects": [dict(row) for row in effects],
        "observations": [dict(row) for row in observations],
    }


def assemble_report(args: argparse.Namespace) -> None:
    control, receiver, observer = Path(args.control), Path(args.receiver), Path(args.observer)
    state = load_state(control, receiver, observer)
    if state["admission"] is None or state["claim"] is None:
        fail("source report requires admission and claim")
    if len(state["attempts"]) != 1 or len(state["effects"]) != 1 or len(state["observations"]) != 2:
        fail("source report requires 1 attempt, 1 effect, 2 observations")

    admission = state["admission"]
    attempt = state["attempts"][0]
    effect = state["effects"][0]
    claim = state["claim"]
    unavailable, full = state["observations"]

    timeline_ns = [
        int(admission["admitted_at_ns"]),
        int(admission["dispatch_started_at_ns"]),
        int(effect["committed_at_ns"]),
        int(claim["claimed_at_ns"]),
        int(unavailable["observed_at_ns"]),
        int(full["observed_at_ns"]),
    ]
    if timeline_ns != sorted(timeline_ns) or len(set(timeline_ns)) != len(timeline_ns):
        fail("source timestamps are not strictly increasing")

    report = {
        "schema": "contractgraph.knowledge-lag-source.v0.1",
        "system_case": SYSTEM_CASE,
        "status": "OBSERVED",
        "action": {
            "action_id": ACTION_ID,
            "target": TARGET,
            "payload_digest": PAYLOAD_DIGEST,
            "request": REQUEST,
        },
        "timeline": {
            "T0_admitted": {
                "ns": admission["admitted_at_ns"],
                "utc": admission["admitted_at_utc"],
            },
            "T0D_dispatch_started": {
                "ns": admission["dispatch_started_at_ns"],
                "utc": admission["dispatch_started_at_utc"],
            },
            "T1_effect_committed": {
                "ns": effect["committed_at_ns"],
                "utc": effect["committed_at_utc"],
            },
            "T2_claim_success": {
                "ns": claim["claimed_at_ns"],
                "utc": claim["claimed_at_utc"],
            },
            "T3_readback_unavailable": {
                "ns": unavailable["observed_at_ns"],
                "utc": unavailable["observed_at_utc"],
            },
            "T4_readback_full": {
                "ns": full["observed_at_ns"],
                "utc": full["observed_at_utc"],
            },
        },
        "admission": {
            "authority_consumed": bool(admission["authority_consumed"]),
            "precedes_dispatch": int(admission["admitted_at_ns"]) < int(admission["dispatch_started_at_ns"]),
        },
        "receiver": {
            "attempt_count": len(state["attempts"]),
            "effect_count": len(state["effects"]),
            "effect": {
                "effect_id": effect["effect_id"],
                "action_id": effect["action_id"],
                "target": effect["target"],
                "payload_digest": effect["payload_digest"],
                "result": effect["result"],
                "committed_at_ns": effect["committed_at_ns"],
                "committed_at_utc": effect["committed_at_utc"],
            },
        },
        "claim": {
            "value": claim["claim"],
            "claimed_at_ns": claim["claimed_at_ns"],
            "claimed_at_utc": claim["claimed_at_utc"],
        },
        "observations": [
            {
                "revision": unavailable["revision"],
                "availability": unavailable["availability"],
                "external_outcome": unavailable["external_outcome"],
                "externally_verified": bool(unavailable["externally_verified"]),
                "effect_count": unavailable["effect_count"],
                "observed_effect_id": unavailable["observed_effect_id"],
                "observed_at_ns": unavailable["observed_at_ns"],
                "observed_at_utc": unavailable["observed_at_utc"],
            },
            {
                "revision": full["revision"],
                "availability": full["availability"],
                "external_outcome": full["external_outcome"],
                "externally_verified": bool(full["externally_verified"]),
                "effect_count": full["effect_count"],
                "observed_effect_id": full["observed_effect_id"],
                "observed_effect_committed_at_ns": full["observed_effect_committed_at_ns"],
                "observed_effect_committed_at_utc": full["observed_effect_committed_at_utc"],
                "observed_at_ns": full["observed_at_ns"],
                "observed_at_utc": full["observed_at_utc"],
            },
        ],
        "source_db_sha256": {
            "control": hashlib.sha256(control.read_bytes()).hexdigest(),
            "receiver": hashlib.sha256(receiver.read_bytes()).hexdigest(),
            "observer": hashlib.sha256(observer.read_bytes()).hexdigest(),
        },
        "claim_ceiling":
            "Same-host independent SQLite authority fixture. Receiver time is source-valid time for this bounded effect only; no distributed clock or production-provider claim.",
    }
    write_json(Path(args.report), report)
    print(json.dumps({"system_case": SYSTEM_CASE, "status": "SOURCE_REPORT_WRITTEN", "effect_valid_time": effect["committed_at_utc"]}, indent=2))


def load_fixture(path: Path) -> dict[str, Any]:
    fixture = json.loads(path.read_text(encoding="utf-8"))
    if fixture.get("system_case") != SYSTEM_CASE:
        fail("wrong fixture system_case")
    return fixture


def revision_from_fixture(fixture: dict[str, Any], knowledge_state: str) -> dict[str, Any]:
    for revision in fixture["expected"]["revisions"]:
        if revision["knowledge_state"] == knowledge_state:
            return revision
    fail(f"fixture missing knowledge state {knowledge_state}")


def sql_text(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def sql_bool(value: bool | None) -> str:
    if value is None:
        return "CAST(NULL AS BOOLEAN)"
    return "TRUE" if value else "FALSE"


def sql_int(value: int | None) -> str:
    return "CAST(NULL AS BIGINT)" if value is None else str(value)


def sql_timestamp(value: str) -> str:
    return "TIMESTAMP " + sql_text(value.replace("+00:00", "Z"))


XTDB_COLUMNS = [
    "_id",
    "case_id",
    "target",
    "payload_digest",
    "knowledge_state",
    "source_event_time",
    "effect_committed_at",
    "client_claim",
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
    "_valid_from",
]


def emit_xtdb(args: argparse.Namespace) -> None:
    fixture = load_fixture(Path(args.fixture))
    state = load_state(Path(args.control), Path(args.receiver), Path(args.observer))
    if len(state["effects"]) != 1:
        fail("XTDB phase requires receiver effect to exist")
    effect = state["effects"][0]
    claim = state["claim"]
    observations = state["observations"]

    phase = args.phase
    if phase == "claim":
        if claim is None:
            fail("claim XTDB phase requires source claim")
        knowledge_state = "CLAIM_ONLY"
        source_event_time = claim["claimed_at_utc"]
        source = revision_from_fixture(fixture, knowledge_state)["source"]
        liminal = revision_from_fixture(fixture, knowledge_state)["liminal"]
    elif phase == "unavailable":
        if len(observations) < 1:
            fail("unavailable XTDB phase requires first observer record")
        obs = observations[0]
        if obs["availability"] != "UNAVAILABLE":
            fail("first observer record is not UNAVAILABLE")
        knowledge_state = "READBACK_UNAVAILABLE"
        source_event_time = obs["observed_at_utc"]
        source = revision_from_fixture(fixture, knowledge_state)["source"]
        liminal = revision_from_fixture(fixture, knowledge_state)["liminal"]
    elif phase == "full":
        if len(observations) < 2:
            fail("full XTDB phase requires second observer record")
        obs = observations[1]
        if obs["availability"] != "FULL":
            fail("second observer record is not FULL")
        knowledge_state = "READBACK_FULL"
        source_event_time = obs["observed_at_utc"]
        source = revision_from_fixture(fixture, knowledge_state)["source"]
        liminal = revision_from_fixture(fixture, knowledge_state)["liminal"]
    else:
        fail(f"unsupported XTDB phase {phase}")

    values = [
        sql_text(ACTION_ID),
        sql_text("retroactive_readback"),
        sql_text(TARGET),
        sql_text(PAYLOAD_DIGEST),
        sql_text(knowledge_state),
        sql_timestamp(source_event_time),
        sql_timestamp(effect["committed_at_utc"]),
        sql_text("SUCCESS"),
        sql_text(source["observation_availability"]),
        sql_text(source["external_outcome"]),
        sql_bool(source["externally_verified"]),
        sql_int(source["effect_count"]),
        sql_text(liminal["authority"]),
        sql_text(liminal["execution"]),
        sql_text(liminal["response_integrity"]),
        sql_text(liminal["causal_validity"]),
        sql_text(liminal["continuity_posture"]),
        sql_bool(liminal["side_effect_committed"]),
        sql_timestamp(effect["committed_at_utc"]),
    ]
    sql = (
        f"INSERT INTO {XTDB_TABLE} (" + ", ".join(XTDB_COLUMNS) + ") VALUES\n  ("
        + ", ".join(values)
        + ");\n"
    )
    Path(args.output).write_text(sql, encoding="utf-8")
    print(json.dumps({"phase": phase, "knowledge_state": knowledge_state, "valid_time": effect["committed_at_utc"], "source_event_time": source_event_time}, indent=2))


def cypher_string(value: str) -> str:
    return "'" + value.replace("\\", "\\\\").replace("'", "\\'") + "'"


def cypher_bool(value: bool) -> str:
    return "true" if value else "false"


def cypher_props(alias: str, props: dict[str, Any]) -> str:
    parts = []
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


def build_final(args: argparse.Namespace) -> None:
    fixture = load_fixture(Path(args.fixture))
    report = json.loads(Path(args.source_report).read_text(encoding="utf-8"))
    if report.get("system_case") != SYSTEM_CASE or report.get("status") != "OBSERVED":
        fail("source report not ready")

    timeline = report["timeline"]
    revisions = []
    revision_times = [
        ("CLAIM_ONLY", timeline["T2_claim_success"]["utc"]),
        ("READBACK_UNAVAILABLE", timeline["T3_readback_unavailable"]["utc"]),
        ("READBACK_FULL", timeline["T4_readback_full"]["utc"]),
    ]
    for index, (state_name, event_time) in enumerate(revision_times, start=1):
        mapped = revision_from_fixture(fixture, state_name)
        revisions.append(
            {
                "revision": index,
                "knowledge_state": state_name,
                "source_event_time_utc": event_time,
                "source": copy.deepcopy(mapped["source"]),
                "liminal": copy.deepcopy(mapped["liminal"]),
            }
        )

    envelope = {
        "schema": ENVELOPE_SCHEMA,
        "system_case": SYSTEM_CASE,
        "action": copy.deepcopy(report["action"]),
        "timeline": copy.deepcopy(timeline),
        "effect": copy.deepcopy(report["receiver"]["effect"]),
        "source_report_sha256": hashlib.sha256(Path(args.source_report).read_bytes()).hexdigest(),
        "revisions": revisions,
        "temporal_contract": {
            "valid_time": timeline["T1_effect_committed"]["utc"],
            "knowledge_time_rule":
                "XTDB system-time is assigned when each knowledge revision is inserted live; source observer times T2/T3/T4 remain separate source-event timestamps.",
            "invariant":
                "Later FULL readback may revise knowledge about T1, but a query AS OF the earlier system-time must preserve the earlier INDETERMINATE state.",
        },
        "claim_ceiling": fixture["claim_ceiling"],
    }
    unsigned = copy.deepcopy(envelope)
    envelope["envelope_sha256"] = sha256_json(unsigned)
    write_json(Path(args.envelope), envelope)

    # Neo4j is a read-only explanation graph. Receiver Effect existence is distinct from observation visibility.
    lines = [
        "CREATE CONSTRAINT knowledge_action_unique IF NOT EXISTS FOR (n:Action) REQUIRE n.action_id IS UNIQUE;",
        "CREATE CONSTRAINT knowledge_effect_unique IF NOT EXISTS FOR (n:Effect) REQUIRE n.effect_id IS UNIQUE;",
        "CREATE CONSTRAINT knowledge_claim_unique IF NOT EXISTS FOR (n:Claim) REQUIRE n.claim_id IS UNIQUE;",
        "CREATE CONSTRAINT knowledge_observation_unique IF NOT EXISTS FOR (n:Observation) REQUIRE n.observation_id IS UNIQUE;",
        "CREATE CONSTRAINT knowledge_continuity_unique IF NOT EXISTS FOR (n:Continuity) REQUIRE n.continuity_id IS UNIQUE;",
        f"MERGE (a:Action {{action_id:{cypher_string(ACTION_ID)}}}) SET {cypher_props('a', {'target': TARGET, 'payload_digest': PAYLOAD_DIGEST})};",
        f"MERGE (e:Effect {{effect_id:{cypher_string(EFFECT_ID)}}}) SET {cypher_props('e', {'action_id': ACTION_ID, 'target': TARGET, 'payload_digest': PAYLOAD_DIGEST, 'committed_at': timeline['T1_effect_committed']['utc'], 'receiver_truth': True})};",
        f"MATCH (a:Action {{action_id:{cypher_string(ACTION_ID)}}}), (e:Effect {{effect_id:{cypher_string(EFFECT_ID)}}}) MERGE (a)-[:HAS_RECEIVER_EFFECT]->(e);",
        f"MERGE (cl:Claim {{claim_id:{cypher_string(ACTION_ID + ':claim')}}}) SET {cypher_props('cl', {'value': 'SUCCESS', 'claimed_at': timeline['T2_claim_success']['utc']})};",
        f"MATCH (a:Action {{action_id:{cypher_string(ACTION_ID)}}}), (cl:Claim {{claim_id:{cypher_string(ACTION_ID + ':claim')}}}) MERGE (a)-[:HAS_CLAIM]->(cl);",
    ]
    previous_observation = None
    for revision in revisions[1:]:  # Neo4j observation nodes represent actual readback attempts T3/T4.
        idx = revision["revision"]
        obs_id = f"{ACTION_ID}:observation:{idx}"
        cont_id = f"{ACTION_ID}:continuity:{idx}"
        source = revision["source"]
        liminal = revision["liminal"]
        lines.append(
            f"MERGE (o:Observation {{observation_id:{cypher_string(obs_id)}}}) SET "
            + cypher_props(
                "o",
                {
                    "revision": idx,
                    "availability": source["observation_availability"],
                    "external_outcome": source["external_outcome"],
                    "externally_verified": source["externally_verified"],
                    "effect_count_known": source["effect_count"] is not None,
                    "effect_count": source["effect_count"],
                    "observed_at": revision["source_event_time_utc"],
                },
            )
            + ";"
        )
        lines.append(
            f"MATCH (a:Action {{action_id:{cypher_string(ACTION_ID)}}}), (o:Observation {{observation_id:{cypher_string(obs_id)}}}) MERGE (a)-[:HAS_OBSERVATION]->(o);"
        )
        if previous_observation:
            lines.append(
                f"MATCH (old:Observation {{observation_id:{cypher_string(previous_observation)}}}), (new:Observation {{observation_id:{cypher_string(obs_id)}}}) MERGE (old)-[:NEXT_KNOWLEDGE]->(new);"
            )
        previous_observation = obs_id
        if source["observation_availability"] == "FULL" and source["effect_count"] == 1:
            lines.append(
                f"MATCH (o:Observation {{observation_id:{cypher_string(obs_id)}}}), (e:Effect {{effect_id:{cypher_string(EFFECT_ID)}}}) MERGE (o)-[:OBSERVES]->(e);"
            )
            lines.append(
                f"MATCH (cl:Claim {{claim_id:{cypher_string(ACTION_ID + ':claim')}}}), (o:Observation {{observation_id:{cypher_string(obs_id)}}}) MERGE (cl)-[:SUPPORTED_BY]->(o);"
            )
        cont_props = {
            "revision": idx,
            "authority": liminal["authority"],
            "execution": liminal["execution"],
            "response_integrity": liminal["response_integrity"],
            "causal_validity": liminal["causal_validity"],
            "posture": liminal["continuity_posture"],
        }
        lines.append(
            f"MERGE (c:Continuity {{continuity_id:{cypher_string(cont_id)}}}) SET {cypher_props('c', cont_props)};"
        )
        lines.append(
            f"MATCH (o:Observation {{observation_id:{cypher_string(obs_id)}}}), (c:Continuity {{continuity_id:{cypher_string(cont_id)}}}) MERGE (o)-[:INFORMS]->(c);"
        )

    Path(args.neo4j_load).write_text("\n".join(lines) + "\n", encoding="utf-8")
    checks = [
        ("effect_exists", "MATCH (e:Effect) RETURN count(e)", 1),
        (
            "unavailable_observes_zero",
            "MATCH (o:Observation {availability:'UNAVAILABLE'})-[:OBSERVES]->(e:Effect) RETURN count(e)",
            0,
        ),
        (
            "unavailable_revalidates",
            "MATCH (o:Observation {availability:'UNAVAILABLE'})-[:INFORMS]->(c:Continuity) WHERE o.external_outcome='INDETERMINATE' AND c.posture='REVALIDATE' RETURN count(c)",
            1,
        ),
        (
            "full_observes_effect",
            "MATCH (o:Observation {availability:'FULL'})-[:OBSERVES]->(e:Effect) RETURN count(e)",
            1,
        ),
        (
            "full_report_only",
            "MATCH (o:Observation {availability:'FULL'})-[:INFORMS]->(c:Continuity) WHERE o.external_outcome='ONE_EFFECT_MATCHING' AND c.posture='REPORT_ONLY' RETURN count(c)",
            1,
        ),
        (
            "claim_supported_only_by_full",
            "MATCH (:Claim)-[:SUPPORTED_BY]->(o:Observation) WHERE o.availability='FULL' RETURN count(o)",
            1,
        ),
    ]
    verify_lines = [
        f"CALL {{ {query} AS actual }} RETURN {cypher_string('CHECK|' + name + '|')} + toString(actual) + {cypher_string('|' + str(expected))} AS result;"
        for name, query, expected in checks
    ]
    Path(args.neo4j_verify).write_text("\n".join(verify_lines) + "\n", encoding="utf-8")
    print(json.dumps({"status": "FINAL_BUILD_READY", "envelope_sha256": envelope["envelope_sha256"], "neo4j_checks": len(checks)}, indent=2))


def read_csv(path: Path) -> list[dict[str, str]]:
    with path.open(newline="", encoding="utf-8") as handle:
        return list(csv.DictReader(handle, skipinitialspace=True))


def parse_ts(value: str) -> datetime:
    value = value.strip().replace(" ", "T")
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    if value.endswith("+00"):
        value += ":00"
    return datetime.fromisoformat(value)


def verify(args: argparse.Namespace) -> None:
    envelope = json.loads(Path(args.envelope).read_text(encoding="utf-8"))
    unsigned = copy.deepcopy(envelope)
    supplied = unsigned.pop("envelope_sha256")
    if sha256_json(unsigned) != supplied:
        fail("envelope semantic SHA mismatch")

    history = read_csv(Path(args.xtdb_history))
    current = read_csv(Path(args.xtdb_current))
    witness_claim = read_csv(Path(args.witness_claim))
    witness_unavailable = read_csv(Path(args.witness_unavailable))
    witness_unavailable_after = read_csv(Path(args.witness_unavailable_after))
    witness_full = read_csv(Path(args.witness_full))
    neo = read_csv(Path(args.neo4j_rows))

    if len(current) != 1 or len(history) != 3:
        fail(f"XTDB current/history row count drift: {len(current)}/{len(history)}")

    history.sort(key=lambda row: row["_system_from"])
    expected_states = ["CLAIM_ONLY", "READBACK_UNAVAILABLE", "READBACK_FULL"]
    actual_states = [row["knowledge_state"] for row in history]
    if actual_states != expected_states:
        fail(f"XTDB knowledge sequence drift: {actual_states}")

    valid_times = {row["_valid_from"] for row in history}
    if len(valid_times) != 1:
        fail("XTDB revisions do not share one effect valid-time")
    source_valid = parse_ts(envelope["timeline"]["T1_effect_committed"]["utc"])
    xtdb_valid = parse_ts(next(iter(valid_times)))
    if source_valid != xtdb_valid:
        fail(f"XTDB valid-time {xtdb_valid} != receiver effect time {source_valid}")

    for rows, expected_state, expected_outcome in (
        (witness_claim, "CLAIM_ONLY", "INDETERMINATE"),
        (witness_unavailable, "READBACK_UNAVAILABLE", "INDETERMINATE"),
        (witness_unavailable_after, "READBACK_UNAVAILABLE", "INDETERMINATE"),
        (witness_full, "READBACK_FULL", "ONE_EFFECT_MATCHING"),
    ):
        if len(rows) != 1:
            fail(f"witness {expected_state} expected one row, got {len(rows)}")
        row = rows[0]
        if row["knowledge_state"] != expected_state or row["external_outcome"] != expected_outcome:
            fail(f"witness drift: expected {expected_state}/{expected_outcome}, got {row['knowledge_state']}/{row['external_outcome']}")

    if witness_unavailable_after[0]["_system_from"] != witness_unavailable[0]["_system_from"]:
        fail("later insertion rewrote the earlier UNAVAILABLE system-time version")

    if witness_unavailable[0]["liminal_continuity_posture"] != "REVALIDATE":
        fail("UNAVAILABLE witness escaped REVALIDATE")
    if witness_full[0]["liminal_continuity_posture"] != "REPORT_ONLY":
        fail("FULL witness did not reach REPORT_ONLY")

    system_times = [parse_ts(row["_system_from"]) for row in history]
    if not (system_times[0] < system_times[1] < system_times[2]):
        fail("XTDB system-time versions are not strictly increasing")

    source_times = [
        parse_ts(envelope["timeline"]["T0_admitted"]["utc"]),
        parse_ts(envelope["timeline"]["T0D_dispatch_started"]["utc"]),
        parse_ts(envelope["timeline"]["T1_effect_committed"]["utc"]),
        parse_ts(envelope["timeline"]["T2_claim_success"]["utc"]),
        parse_ts(envelope["timeline"]["T3_readback_unavailable"]["utc"]),
        parse_ts(envelope["timeline"]["T4_readback_full"]["utc"]),
    ]
    if source_times != sorted(source_times) or len(set(source_times)) != len(source_times):
        fail("source event times are not strictly ordered")

    # Neo4j explanation rows: T3 hidden effect and T4 visible effect.
    if len(neo) != 2:
        fail(f"Neo4j explanation rows={len(neo)}, expected 2")
    by_availability = {row["availability"]: row for row in neo}
    unavailable = by_availability.get("UNAVAILABLE")
    full = by_availability.get("FULL")
    if unavailable is None or full is None:
        fail("Neo4j missing UNAVAILABLE/FULL observations")
    if int(unavailable["receiver_effect_count"]) != 1 or int(unavailable["observed_effects"]) != 0:
        fail("Neo4j hidden-effect boundary failed at T3")
    if unavailable["posture"] != "REVALIDATE" or unavailable["external_outcome"] != "INDETERMINATE":
        fail("Neo4j T3 conclusion drift")
    if int(full["receiver_effect_count"]) != 1 or int(full["observed_effects"]) != 1:
        fail("Neo4j T4 observation did not bind effect")
    if full["posture"] != "REPORT_ONLY" or full["external_outcome"] != "ONE_EFFECT_MATCHING":
        fail("Neo4j T4 conclusion drift")

    liminal_marker = Path(args.liminal_marker).read_text(encoding="utf-8").strip()
    if liminal_marker != "PASS":
        fail("native LiminalDB marker is not PASS")

    result = {
        "system_case": SYSTEM_CASE,
        "status": "PASS",
        "envelope_sha256": envelope["envelope_sha256"],
        "source_valid_time": envelope["timeline"]["T1_effect_committed"]["utc"],
        "source_observation_times": {
            "claim": envelope["timeline"]["T2_claim_success"]["utc"],
            "unavailable": envelope["timeline"]["T3_readback_unavailable"]["utc"],
            "full": envelope["timeline"]["T4_readback_full"]["utc"],
        },
        "xtdb": {
            "history_rows": 3,
            "knowledge_sequence": expected_states,
            "earlier_unavailable_version_preserved_after_full_insert": True,
            "valid_time_equals_receiver_effect_time": True,
        },
        "liminaldb": "PASS",
        "neo4j": {
            "receiver_effect_exists_at_T3": True,
            "T3_observes_effect": False,
            "T3_posture": "REVALIDATE",
            "T4_observes_effect": True,
            "T4_posture": "REPORT_ONLY",
        },
        "core_result":
            "What happened at T1 and what the system knew before T4 are independently queryable. Later evidence changes the current conclusion without rewriting the earlier knowable state.",
        "claim_ceiling": envelope["claim_ceiling"],
    }
    write_json(Path(args.report), result)
    print(json.dumps(result, indent=2, sort_keys=True))


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser()
    sub = root.add_subparsers(dest="command", required=True)

    for name, func in (
        ("admit", phase_admit),
        ("dispatch", phase_dispatch),
        ("effect", phase_effect),
        ("claim", phase_claim),
        ("unavailable", phase_unavailable),
        ("full", phase_full),
    ):
        p = sub.add_parser(name)
        p.add_argument("--control", required=True)
        p.add_argument("--receiver", required=True)
        p.add_argument("--observer", required=True)
        p.set_defaults(func=func)

    p = sub.add_parser("source-report")
    p.add_argument("--control", required=True)
    p.add_argument("--receiver", required=True)
    p.add_argument("--observer", required=True)
    p.add_argument("--report", required=True)
    p.set_defaults(func=assemble_report)

    p = sub.add_parser("xtdb-phase")
    p.add_argument("--phase", choices=("claim", "unavailable", "full"), required=True)
    p.add_argument("--control", required=True)
    p.add_argument("--receiver", required=True)
    p.add_argument("--observer", required=True)
    p.add_argument("--fixture", required=True)
    p.add_argument("--output", required=True)
    p.set_defaults(func=emit_xtdb)

    p = sub.add_parser("build-final")
    p.add_argument("--source-report", required=True)
    p.add_argument("--fixture", required=True)
    p.add_argument("--envelope", required=True)
    p.add_argument("--neo4j-load", required=True)
    p.add_argument("--neo4j-verify", required=True)
    p.set_defaults(func=build_final)

    p = sub.add_parser("verify")
    p.add_argument("--envelope", required=True)
    p.add_argument("--xtdb-current", required=True)
    p.add_argument("--xtdb-history", required=True)
    p.add_argument("--witness-claim", required=True)
    p.add_argument("--witness-unavailable", required=True)
    p.add_argument("--witness-unavailable-after", required=True)
    p.add_argument("--witness-full", required=True)
    p.add_argument("--neo4j-rows", required=True)
    p.add_argument("--liminal-marker", required=True)
    p.add_argument("--report", required=True)
    p.set_defaults(func=verify)

    return root


def main() -> None:
    args = parser().parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
