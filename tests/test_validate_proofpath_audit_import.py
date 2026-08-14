from __future__ import annotations

import copy
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "validate_proofpath_audit_import.py"
SPEC = importlib.util.spec_from_file_location("validate_proofpath_audit_import", MODULE_PATH)
assert SPEC and SPEC.loader
validator = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validator)


def valid_event() -> dict:
    event = {
        "id": "proofpath-19c437e47a3c4f1a4e0308cfeb375474",
        "ts": "2026-08-14T07:40:00Z",
        "correlationId": "lop:neo-rezonans:heartbeat:001",
        "kind": "audit",
        "actor": "proofpath-scig-native-verifier",
        "action": "proofpath.scig.verification.observed",
        "details": {
            "schema_version": "liminaldb-proofpath-audit-event-v0.1",
            "logical_operation_id": "lop:neo-rezonans:heartbeat:001",
            "source": {
                "repository": "safal207/ProofPath",
                "capability_id": "proofpath.scig.v0.1",
                "capability_commit": "685d50e256a5125a21f4c4584b326411caaa64ad",
                "incident_id": "CGQA-PROOFPATH-0123456789abcdef",
                "scig_sha256": "1" * 64,
                "native_result": "VALID",
                "native_verifier": "proofpath-scig",
                "bridge_receipt_sha256": "2" * 64,
                "verification_class": "native_recomputed",
            },
            "evidence": {
                "bounded": True,
                "replayable": True,
                "source_receipt_bound": True,
            },
            "authority": {
                "mode": "evidence_only",
                "execution": False,
                "mutation": False,
                "persistence": False,
                "deployment": False,
                "merge": False,
            },
            "persistence": {
                "write_mode": "artifact_only",
                "durable_memory": False,
                "live_ingestion": False,
                "namespace_mutation": False,
            },
            "adapter": {
                "repository": "safal207/LiminalDB",
                "commit": "797a97cacb341798e1e308c368e465246bbf0a15",
                "contract_path": "sdk/ts/src/protocol-types.ts",
                "contract_blob_sha": "fd733971aaae089df770062bcf7f2c2d6d19ca1d",
                "event_contract": "AuditEvent",
                "write_mode": "artifact_only",
            },
        },
    }
    event["details"]["event_sha256"] = validator.sha256_json(event)
    return event


def rewrite_hash(event: dict) -> dict:
    event = copy.deepcopy(event)
    event["details"].pop("event_sha256", None)
    event["details"]["event_sha256"] = validator.sha256_json(event)
    return event


class ProofPathAuditImportTests(unittest.TestCase):
    def test_valid_event_and_summary_remain_non_persistent(self) -> None:
        event = valid_event()
        current_blob = validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE)
        validator.validate_event(event, "event", current_contract_blob_sha=current_blob)
        summary = validator.build_summary([event], consumer_commit="797a97cacb341798e1e308c368e465246bbf0a15")
        self.assertEqual(summary["schema_version"], "liminaldb-proofpath-import-check-v0.1")
        self.assertFalse(summary["write_performed"])
        self.assertFalse(summary["authority"]["durable_memory_accepted"])
        self.assertFalse(summary["authority"]["live_ingestion_performed"])
        self.assertEqual(summary["source"]["verification_class"], "native_recomputed")
        self.assertFalse(summary["compatibility"]["historical_snapshot_is_semantic_key"])

    def test_logical_operation_identity_cannot_drift(self) -> None:
        event = valid_event()
        event["correlationId"] = "lop:other"
        event = rewrite_hash(event)
        with self.assertRaisesRegex(ValueError, "correlationId"):
            validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_native_result_must_be_valid(self) -> None:
        event = valid_event()
        event["details"]["source"]["native_result"] = "INVALID"
        event = rewrite_hash(event)
        with self.assertRaisesRegex(ValueError, "native_result"):
            validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_authority_cannot_leak(self) -> None:
        for field in validator.AUTHORITY_FIELDS:
            with self.subTest(field=field):
                event = valid_event()
                event["details"]["authority"][field] = True
                event = rewrite_hash(event)
                with self.assertRaisesRegex(ValueError, field):
                    validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_artifact_contract_cannot_claim_durable_persistence(self) -> None:
        for field in validator.PERSISTENCE_FALSE_FIELDS:
            with self.subTest(field=field):
                event = valid_event()
                event["details"]["persistence"][field] = True
                event = rewrite_hash(event)
                with self.assertRaisesRegex(ValueError, field):
                    validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_contract_blob_drift_fails_closed(self) -> None:
        event = valid_event()
        event["details"]["adapter"]["contract_blob_sha"] = "0" * 40
        event = rewrite_hash(event)
        with self.assertRaisesRegex(ValueError, "contract_blob_sha"):
            validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_event_integrity_is_checked(self) -> None:
        event = valid_event()
        event["details"]["source"]["incident_id"] = "tampered"
        with self.assertRaisesRegex(ValueError, "event_sha256 mismatch"):
            validator.validate_event(event, "event", current_contract_blob_sha=validator.git_blob_sha(validator.DEFAULT_CONTRACT_FILE))

    def test_duplicate_event_ids_are_rejected(self) -> None:
        event = valid_event()
        with tempfile.TemporaryDirectory() as tempdir:
            path = Path(tempdir) / "events.jsonl"
            line = json.dumps(event, sort_keys=True)
            path.write_text(line + "\n" + line + "\n", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "duplicates event id"):
                validator.load_events(path)


if __name__ == "__main__":
    unittest.main()
