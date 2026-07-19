from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "validate_pythia_negative_memory.py"
SPEC = importlib.util.spec_from_file_location("validate_pythia_negative_memory", MODULE_PATH)
assert SPEC and SPEC.loader
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


def valid_event() -> dict:
    event = {
        "id": "pythia-neg-0123456789abcdef0123456789abcdef",
        "ts": "2026-07-19T23:40:00+03:00",
        "kind": "audit",
        "actor": "pythia-lotus",
        "action": "lotus.verified_negative.observed",
        "details": {
            "schema_version": "liminaldb-pythia-negative-memory-event-v0.1",
            "source": {
                "repository": "safal207/LiminalQAengineer",
                "branch": "agent/garden-dns-live-ci-v0-1",
                "commit": "c7af3e210d007026fedc57eb9435069a958fac6f",
                "packet_id": "airbnb-garden-29702510829",
                "packet_sha256": "a" * 64,
                "authorization_ref": "sha256:" + "1" * 64,
                "observation_ref": "sha256:" + "2" * 64,
                "observation_result_ref": "sha256:" + "3" * 64,
            },
            "judgment": {
                "schema_version": "pythia-lotus-ltp-judgment-v0.1",
                "sha256": "b" * 64,
                "decision_status": "CONFIRMED",
                "pythia_verdict": "ALLOW",
                "result_class": "VERIFIED_NEGATIVE_OBSERVATION",
                "canonical_id": "airbnb.public.currency-history.no-defect-observed",
                "memory_kind": "VERIFIED_NEGATIVE_OBSERVATION",
                "cause_status": "UNCONFIRMED",
                "confidence": "OBSERVED_ONCE",
                "recurrence": "SINGLE_TRANSITION",
                "durable_memory": False,
            },
            "evidence": {
                "bounded": True,
                "replayable": True,
                "ltp_verification_level": "FULL_LIFECYCLE_JOINED",
                "response_integrity": "VERIFIED",
                "fabricated_claim_control": "CONTRADICTED",
            },
            "authority": {
                "mode": "audit_only",
                "ownership": False,
                "approval": False,
                "execution": False,
                "delivery": False,
                "external_submission": False,
                "deployment": False,
                "merge": False,
                "durable_memory_write": False,
            },
            "adapter": {
                "repository": "safal207/pythiaLabs",
                "commit": "92b4ade7c2057f6d5cf542dca1bf474360cd74c1",
                "event_contract": "AuditEvent-extension",
                "write_mode": "artifact_only",
            },
        },
    }
    event["details"]["event_sha256"] = validator.sha256_json(event)
    return event


class ValidatePythiaNegativeMemoryTest(unittest.TestCase):
    def write_events(self, events: list[dict]) -> Path:
        tmp = tempfile.NamedTemporaryFile("w", suffix=".jsonl", delete=False)
        with tmp:
            for event in events:
                tmp.write(json.dumps(event, sort_keys=True) + "\n")
        return Path(tmp.name)

    def test_valid_event_is_dry_run_only(self) -> None:
        event = valid_event()
        validated = validator.load_events(self.write_events([event]))
        summary = validator.build_summary(validated)
        self.assertEqual(summary["verified_negative_count"], 1)
        self.assertFalse(summary["write_performed"])
        self.assertFalse(summary["authority"]["durable_memory_accepted"])
        self.assertFalse(summary["authority"]["live_ingestion_performed"])

    def test_event_hash_tampering_is_rejected(self) -> None:
        event = valid_event()
        event["details"]["source"]["packet_id"] = "tampered"
        with self.assertRaisesRegex(ValueError, "event_sha256 mismatch"):
            validator.load_events(self.write_events([event]))

    def test_durable_memory_claim_is_rejected(self) -> None:
        event = valid_event()
        event["details"]["judgment"]["durable_memory"] = True
        event["details"]["event_sha256"] = validator.sha256_json({**event, "details": {k: v for k, v in event["details"].items() if k != "event_sha256"}})
        with self.assertRaisesRegex(ValueError, "durable_memory"):
            validator.load_events(self.write_events([event]))

    def test_ltp_level_downgrade_is_rejected(self) -> None:
        event = valid_event()
        event["details"]["evidence"]["ltp_verification_level"] = "TEXT_HEURISTIC"
        event["details"]["event_sha256"] = validator.sha256_json({**event, "details": {k: v for k, v in event["details"].items() if k != "event_sha256"}})
        with self.assertRaisesRegex(ValueError, "ltp_verification_level"):
            validator.load_events(self.write_events([event]))

    def test_authority_escalation_is_rejected(self) -> None:
        event = valid_event()
        event["details"]["authority"]["durable_memory_write"] = True
        event["details"]["event_sha256"] = validator.sha256_json({**event, "details": {k: v for k, v in event["details"].items() if k != "event_sha256"}})
        with self.assertRaisesRegex(ValueError, "durable_memory_write"):
            validator.load_events(self.write_events([event]))

    def test_wrong_pythia_head_is_rejected(self) -> None:
        event = valid_event()
        event["details"]["adapter"]["commit"] = "0" * 40
        event["details"]["event_sha256"] = validator.sha256_json({**event, "details": {k: v for k, v in event["details"].items() if k != "event_sha256"}})
        with self.assertRaisesRegex(ValueError, "adapter.commit"):
            validator.load_events(self.write_events([event]))

    def test_duplicate_event_id_is_rejected(self) -> None:
        event = valid_event()
        with self.assertRaisesRegex(ValueError, "duplicates event id"):
            validator.load_events(self.write_events([event, copy.deepcopy(event)]))


if __name__ == "__main__":
    unittest.main()
