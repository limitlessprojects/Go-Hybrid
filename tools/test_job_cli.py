"""Regression tests for fail-closed CLI argument validation (no network)."""
import contextlib
import importlib.util
import io
import subprocess
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ENTRY = Path(__file__).with_name("job.py")


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


class KaggleWaitStatusTests(unittest.TestCase):
    def wait(self, responses):
        spec = importlib.util.spec_from_file_location("job_wait_test", ENTRY)
        job = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job)
        with patch.object(job, "sh", side_effect=responses), \
             patch.object(job.time, "time", side_effect=range(len(responses) + 2)), \
             patch.object(job.time, "sleep"), \
             contextlib.redirect_stdout(io.StringIO()):
            return job.kaggle_wait((len(responses) + 1) / 60, "owner/kernel")

    def test_failed_poll_cannot_establish_active_state(self):
        self.assertEqual(self.wait([
            (1, 'error referring to "KernelWorkerStatus.RUNNING"'),
            (0, 'status "KernelWorkerStatus.COMPLETE"'),
        ]), 2)

    def test_failed_poll_cannot_report_completion(self):
        self.assertEqual(self.wait([
            (0, 'status "KernelWorkerStatus.RUNNING"'),
            (1, 'error referring to "KernelWorkerStatus.COMPLETE"'),
        ]), 2)

    def test_failed_poll_does_not_end_wait_before_valid_completion(self):
        self.assertEqual(self.wait([
            (0, 'status "KernelWorkerStatus.RUNNING"'),
            (1, 'error referring to "KernelWorkerStatus.ERROR"'),
            (0, 'status "KernelWorkerStatus.COMPLETE"'),
        ]), 0)

    def test_successful_terminal_polls_keep_exit_contract(self):
        for state, expected in [("COMPLETE", 0), ("ERROR", 1),
                                ("CANCELACKNOWLEDGED", 1), ("CANCELEDEXECUTION", 1)]:
            with self.subTest(state=state):
                self.assertEqual(self.wait([
                    (0, 'status "KernelWorkerStatus.QUEUED"'),
                    (0, f'status "KernelWorkerStatus.{state}"'),
                ]), expected)

    def test_stale_completion_still_times_out(self):
        self.assertEqual(self.wait([(0, 'status "KernelWorkerStatus.COMPLETE"')]), 2)


if __name__ == "__main__":
    unittest.main()
