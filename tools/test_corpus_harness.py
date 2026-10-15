"""CLI regressions for optional exact diagnostic metadata (stdlib only)."""

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

HARNESS = Path(__file__).with_name("corpus_harness.py")


class ExactDiagnosticCliTests(unittest.TestCase):
    def run_harness(self, header, code="HYB-E3002", verdict="REJECT A1c"):
        # A tiny deterministic checker exercises the complete harness CLI:
        # fixture metadata -> subprocess JSON -> rule/code comparison -> exit.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                f"// EXPECTED-VERDICT: {verdict}\n{header}\nfn main() {{}}\n",
                encoding="utf-8",
            )
            payload = {
                "summary": {"verdict": "reject"},
                "diagnostics": [{"severity": "error", "tier0_rule": "A1c", "code": code}],
            }
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit(1)\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def test_wrong_code_with_same_rule_fails(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002", code="HYB-E3001")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("expected first diagnostic HYB-E3002, got HYB-E3001", run.stdout)
        self.assertIn("verdict agreement: 0/1", run.stdout)

    def test_exact_code_match_passes(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_old_fixture_without_header_keeps_rule_only_contract(self):
        run = self.run_harness("", code="HYB-E3001")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_malformed_header_fails_closed(self):
        for header in (
            "// EXPECTED-DIAGNOSTIC: not-a-code",
            "// EXPECTED-DIAGNOSTIC HYB-E3002",
            "// EXPECTED-DIAGNOSTIC:",
        ):
            with self.subTest(header=header):
                run = self.run_harness(header)
                self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
                self.assertIn("malformed EXPECTED-DIAGNOSTIC", run.stderr)

    def test_duplicate_headers_fail_closed(self):
        run = self.run_harness(
            "// EXPECTED-DIAGNOSTIC: HYB-E3002\n// EXPECTED-DIAGNOSTIC: HYB-E3002"
        )
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("multiple EXPECTED-DIAGNOSTIC", run.stderr)

    def test_accept_fixture_cannot_declare_error_code(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002", verdict="ACCEPT")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("without REJECT", run.stderr)


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
