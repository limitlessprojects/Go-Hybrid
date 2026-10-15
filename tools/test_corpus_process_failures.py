"""A failed checker process must not prevent remaining corpus cases running."""

import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import corpus_harness


class CheckerProcessFailures(unittest.TestCase):
    def test_timeout_is_a_reported_failure_and_next_case_runs(self):
        self.check_failure(subprocess.TimeoutExpired(["hyb"], 60), "timed out")

    def test_launch_error_is_a_reported_failure_and_next_case_runs(self):
        self.check_failure(PermissionError("execution denied"), "could not start")

    def check_failure(self, failure, message):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            checker = root / "hyb"
            checker.touch()
            for name in ("01-first.hy", "02-next.hy"):
                (root / name).write_text(
                    "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
                )
            accepted = subprocess.CompletedProcess(
                ["hyb"], 0,
                json.dumps({"summary": {"verdict": "accept"}, "diagnostics": []}), ""
            )
            output = io.StringIO()
            with patch.object(sys, "argv", ["corpus_harness.py", "--bin", str(checker), "--corpus", tmp]), \
                 patch.object(corpus_harness.subprocess, "run", side_effect=[failure, accepted]) as run, \
                 contextlib.redirect_stdout(output):
                status = corpus_harness.main()
            self.assertEqual(status, 1)
            self.assertEqual(run.call_count, 2)
            self.assertIn(message, output.getvalue())
            self.assertIn("verdict agreement: 1/2", output.getvalue())


if __name__ == "__main__":
    unittest.main()
