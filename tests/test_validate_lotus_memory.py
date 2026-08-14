from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "validate_lotus_memory.py"
FIXTURE = ROOT / "fixtures" / "lotus" / "valid-events.jsonl"
CONTRACT = ROOT / "sdk" / "ts" / "src" / "protocol-types.ts"
HISTORICAL_CONTRACT_BLOB = "fd733971aaae089df770062bcf7f2c2d6d19ca1d"
HISTORICAL_ADAPTER_COMMIT = "75ef9f7f403a34c60aa2ceba4cb3c97870d73e77"

spec = importlib.util.spec_from_file_location("validate_lotus_memory", SCRIPT)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)


class LotusMemoryImportContractTest(unittest.TestCase):
    def load_fixture(self):
        return [
            json.loads(line)
            for line in FIXTURE.read_text(encoding="utf-8").splitlines()
            if line.strip()
        ]

    def write_events(self, events):
        handle = tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False)
        with handle:
            for event in events:
                handle.write(json.dumps(event, sort_keys=True) + "\n")
        return Path(handle.name)

    def rehash(self, event):
        event["details"].pop("event_sha256", None)
        event["details"]["event_sha256"] = module.sha256_json(event)

    def test_current_checkout_preserves_historical_contract_blob_identity(self):
        self.assertEqual(module.git_blob_sha(CONTRACT), HISTORICAL_CONTRACT_BLOB)

    def test_valid_historical_fixture_is_compatible_with_current_checkout(self):
        events = module.load_events(FIXTURE)
        summary = module.build_summary(
            events,
            consumer_commit="0cd6e77d52787bb36a97b75ba1a37cb027268eb3",
        )
        self.assertEqual(summary["schema_version"], "liminaldb-lotus-import-check-v0.2")
        self.assertEqual(summary["event_count"], 2)
        self.assertEqual(summary["canonical_finding_count"], 2)
        self.assertEqual(summary["mode"], "dry_run")
        self.assertIs(summary["write_performed"], False)
        self.assertIs(summary["authority"]["durable_memory_accepted"], False)
        self.assertEqual(
            summary["compatibility"]["current_contract_blob_sha"],
            HISTORICAL_CONTRACT_BLOB,
        )
        self.assertEqual(
            summary["compatibility"]["declared_adapter_commits"],
            [HISTORICAL_ADAPTER_COMMIT],
        )
        self.assertIs(
            summary["compatibility"]["historical_snapshot_is_semantic_key"],
            False,
        )
        self.assertIs(
            summary["compatibility"]["contract_blob_matches_current_checkout"],
            True,
        )

    def test_historical_adapter_commit_is_provenance_not_current_commit_requirement(self):
        events = self.load_fixture()
        events[0]["details"]["adapter"]["commit"] = "1111111111111111111111111111111111111111"
        self.rehash(events[0])
        path = self.write_events(events)

        accepted = module.load_events(path)
        self.assertEqual(
            accepted[0]["details"]["adapter"]["commit"],
            "1111111111111111111111111111111111111111",
        )

    def test_contract_blob_mismatch_is_rejected_even_with_valid_event_hash(self):
        events = self.load_fixture()
        events[0]["details"]["adapter"]["contract_blob_sha"] = "1" * 40
        self.rehash(events[0])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "incompatible with current checked-out contract"):
            module.load_events(path)

    def test_malformed_historical_adapter_commit_is_rejected(self):
        events = self.load_fixture()
        events[0]["details"]["adapter"]["commit"] = "main"
        self.rehash(events[0])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "full historical commit SHA"):
            module.load_events(path)

    def test_hash_mismatch_is_rejected(self):
        events = self.load_fixture()
        events[0]["details"]["finding"]["surface"] = "tampered"
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "event_sha256 mismatch"):
            module.load_events(path)

    def test_authority_grant_is_rejected_even_with_valid_hash(self):
        events = self.load_fixture()
        events[0]["details"]["authority"]["execution"] = True
        self.rehash(events[0])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "execution must be false"):
            module.load_events(path)

    def test_duplicate_event_id_is_rejected(self):
        events = self.load_fixture()
        events[1]["id"] = events[0]["id"]
        self.rehash(events[1])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "duplicates event id"):
            module.load_events(path)

    def test_non_artifact_write_mode_is_rejected(self):
        events = self.load_fixture()
        events[0]["details"]["adapter"]["write_mode"] = "live"
        self.rehash(events[0])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "write_mode must equal artifact_only"):
            module.load_events(path)

    def test_timestamp_without_timezone_is_rejected(self):
        events = self.load_fixture()
        events[0]["ts"] = "2026-07-19T00:00:00"
        self.rehash(events[0])
        path = self.write_events(events)
        with self.assertRaisesRegex(ValueError, "explicit timezone"):
            module.load_events(path)

    def test_consumer_commit_must_be_exact_sha_when_reported(self):
        events = module.load_events(FIXTURE)
        with self.assertRaisesRegex(ValueError, "consumer_commit"):
            module.build_summary(events, consumer_commit="main")


if __name__ == "__main__":
    unittest.main()
