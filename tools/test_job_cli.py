"""Regression tests for fail-closed CLI argument validation (no network)."""
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ENTRY = Path(__file__).with_name("job.py")

JOB_SPEC = importlib.util.spec_from_file_location("job_module", ENTRY)
JOB = importlib.util.module_from_spec(JOB_SPEC)
JOB_SPEC.loader.exec_module(JOB)


class ActionsDispatchArgumentTests(unittest.TestCase):
    def call_dispatch(self, *args):
        return subprocess.run(
            [sys.executable, str(ENTRY), "actions", "dispatch", *args],
            capture_output=True,
            text=True,
            timeout=5,
        )

    def test_missing_parallelism_reports_usage_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "3")
        self.assertEqual(result.returncode, 2)
        self.assertIn("<maxparallel>", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_invalid_parallelism_reports_error_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "3", "bad")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integers", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_nonpositive_values_are_rejected(self):
        for shards, parallel in [("0", "2"), ("3", "-1"), ("-2", "1")]:
            with self.subTest(shards=shards, parallel=parallel):
                result = self.call_dispatch("dummy.yml", "0123456", "demo", shards, parallel)
                self.assertEqual(result.returncode, 2)
                self.assertIn("positive integers", result.stderr)
                self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_invalid_shards_reports_error_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "bad", "2")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integers", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)


class FetchFailureTests(unittest.TestCase):
    """A fetch that retrieves nothing must exit non-zero, never 0."""

    def test_kaggle_fetch_failed_download_is_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            with patch.object(JOB, "sh", return_value=(1, "kaggle: not authorized")):
                rc = JOB.kaggle_fetch(td, slug="demo/kernel")
            self.assertEqual(rc, 1)

    def test_kaggle_fetch_missing_summary_is_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            with patch.object(JOB, "sh", return_value=(0, "downloaded nothing")):
                rc = JOB.kaggle_fetch(td, slug="demo/kernel")
            self.assertEqual(rc, 1)

    def test_kaggle_fetch_present_summary_is_zero(self):
        with tempfile.TemporaryDirectory() as td:
            raw = Path(td) / "result" / "raw" / "nested"
            raw.mkdir(parents=True)
            (raw / "summary.json").write_text(json.dumps({"ok": True}), encoding="utf-8")
            with patch.object(JOB, "sh", return_value=(0, "ok")):
                rc = JOB.kaggle_fetch(td, slug="demo/kernel")
            self.assertEqual(rc, 0)
            self.assertTrue((Path(td) / "result" / "summary.json").exists())

    def test_actions_fetch_empty_artifact_list_is_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            with patch.object(JOB, "gh_api", return_value={"total_count": 0, "artifacts": []}):
                rc = JOB.actions_fetch(123, td)
            self.assertEqual(rc, 1)

    def test_actions_fetch_error_payload_is_nonzero(self):
        with tempfile.TemporaryDirectory() as td:
            with patch.object(JOB, "gh_api", return_value={"message": "Not Found", "status": "404"}):
                rc = JOB.actions_fetch(123, td)
            self.assertEqual(rc, 1)


if __name__ == "__main__":
    unittest.main()
