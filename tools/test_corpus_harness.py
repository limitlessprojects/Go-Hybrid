"""CLI regressions for optional exact diagnostic metadata (stdlib only)."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

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


class CheckerEnvelopeCliTests(unittest.TestCase):
    def run_payload(self, payload, expected="ACCEPT", status=0):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                f"// EXPECTED-VERDICT: {expected}\nfn main() {{}}\n", encoding="utf-8"
            )
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit({status})\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def assert_invalid(self, payload, expected="ACCEPT", status=0):
        run = self.run_payload(payload, expected, status)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("harness-error", run.stdout)
        self.assertIn("verdict agreement: 0/1", run.stdout)
        self.assertNotIn("Traceback", run.stderr)

    def test_matching_counts_accept(self):
        payload = {"summary": {"verdict": "accept", "errors": 0, "warnings": 1},
                   "diagnostics": [{"severity": "warning", "code": "HYB-E8001"}]}
        run = self.run_payload(payload, expected="ACCEPT-MANAGED")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_matching_counts_reject(self):
        payload = {"summary": {"verdict": "reject", "errors": 1, "warnings": 0},
                   "diagnostics": [{"severity": "error", "tier0_rule": "A1c"}]}
        run = self.run_payload(payload, expected="REJECT A1c", status=1)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_omitted_counts_keep_existing_contract(self):
        run = self.run_payload({"summary": {"verdict": "accept"}, "diagnostics": []})
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_reported_errors_must_match_diagnostics(self):
        self.assert_invalid({"summary": {"verdict": "accept", "errors": 1},
                             "diagnostics": []})
        self.assert_invalid({"summary": {"verdict": "reject", "errors": 0},
                             "diagnostics": [{"severity": "error", "tier0_rule": "A1c"}]},
                            expected="REJECT A1c", status=1)

    def test_reported_warnings_must_match_diagnostics(self):
        self.assert_invalid({"summary": {"verdict": "accept", "warnings": 1},
                             "diagnostics": []})

    def test_counts_require_nonnegative_integers(self):
        for count in (False, True, None, "0", 0.0, -1):
            with self.subTest(count=count):
                self.assert_invalid({"summary": {"verdict": "accept", "errors": count},
                                     "diagnostics": []})

    def test_invalid_envelope_shapes_fail_without_traceback(self):
        for payload in (
            None, [], {"summary": None, "diagnostics": []},
            {"summary": [], "diagnostics": []},
            {"summary": {"verdict": "accept"}, "diagnostics": None},
            {"summary": {"verdict": "accept"}, "diagnostics": {}},
            {"summary": {"verdict": "accept"}, "diagnostics": [None]},
            {"summary": {"verdict": "accept"}, "diagnostics": ["error"]},
        ):
            with self.subTest(payload=payload):
                self.assert_invalid(payload)

    def test_extra_fields_do_not_change_acceptance(self):
        run = self.run_payload({"summary": {"verdict": "accept", "errors": 0,
                                            "warnings": 0, "future": "kept"},
                                "diagnostics": [], "version": 1, "future": {}})
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)


if __name__ == "__main__":
    unittest.main()
