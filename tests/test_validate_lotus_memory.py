from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "validate_lotus_memory.py"
FIXTURE = ROOT / "fixtures" / "lotus" / "valid-events.jsonl"

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

    def test_valid_fixture_is_accepted_as_dry_run(self):
        events = module.load_events(FIXTURE)
        summary = module.build_summary(events)
        self.assertEqual(summary["event_count"], 2)
        self.assertEqual(summary["canonical_finding_count"], 2)
        self.assertEqual(summary["mode"], "dry_run")
        self.assertIs(summary["write_performed"], False)
        self.assertIs(summary["authority"]["durable_memory_accepted"], False)

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


if __name__ == "__main__":
    unittest.main()
