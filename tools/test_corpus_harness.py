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


class VerdictMetadataCliTests(unittest.TestCase):
    def run_corpus(self, source, case_payload=None, case_status=0):
        """Run the real CLI beside a passing control, using an offline checker."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-control.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
            )
            (corpus / "02-case.hy").write_text(source, encoding="utf-8")
            accepted = {"summary": {"verdict": "accept"}, "diagnostics": []}
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nfrom pathlib import Path\nimport json, sys\n"
                "is_case = Path(sys.argv[2]).name == '02-case.hy'\n"
                f"payload = {case_payload if case_payload is not None else accepted!r} if is_case else {accepted!r}\n"
                "print(json.dumps(payload))\n"
                f"raise SystemExit({case_status} if is_case else 0)\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def assert_bad_metadata(self, source, message):
        run = self.run_corpus(source)
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn(message, run.stderr)
        self.assertIn("02-case.hy", run.stderr)
        self.assertNotIn("verdict agreement:", run.stdout)
        self.assertNotIn("Traceback", run.stderr)

    def test_indented_malformed_marker_is_not_silently_skipped(self):
        self.assert_bad_metadata(
            "    // EXPECTED-VERDICT: REJET A1\nfn main() {}\n",
            "invalid EXPECTED-VERDICT",
        )

    def test_unknown_verdict_is_not_silently_skipped(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: REJET A1\nfn main() {}\n", "invalid EXPECTED-VERDICT"
        )

    def test_empty_verdict_is_not_silently_skipped(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: \nfn main() {}\n", "invalid EXPECTED-VERDICT"
        )

    def test_suffixed_verdict_is_not_accepted_by_prefix(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: ACCEPT-BROKEN\nfn main() {}\n", "invalid EXPECTED-VERDICT"
        )

    def test_conflicting_verdicts_are_not_reduced_to_first_match(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n",
            "multiple EXPECTED-VERDICT",
        )

    def test_duplicate_verdicts_are_rejected_even_if_identical(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n",
            "multiple EXPECTED-VERDICT",
        )

    def test_reject_rule_cannot_come_from_the_next_line(self):
        self.assert_bad_metadata(
            "// EXPECTED-VERDICT: REJECT\nfn main() {}\n", "REJECT without a rule id"
        )

    def test_valid_accept_and_unregistered_helpers_keep_their_contract(self):
        for source, agreement in (
            ("// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", "2/2"),
            ("// unregistered helper\nfn helper() {}\n", "1/1"),
        ):
            with self.subTest(source=source):
                run = self.run_corpus(source)
                self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                self.assertIn(f"verdict agreement: {agreement}", run.stdout)

    def test_reject_rule_and_inline_explanation_with_tabs_and_crlf(self):
        payload = {"summary": {"verdict": "reject"},
                   "diagnostics": [{"severity": "error", "tier0_rule": "A1b"}]}
        run = self.run_corpus(
            "//\tEXPECTED-VERDICT:\tREJECT\tA1b (move rule)\r\nfn main() {}\r\n",
            payload, 1,
        )
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 2/2", run.stdout)

    def test_valid_managed_verdict_keeps_its_lint_requirement(self):
        source = "// EXPECTED-VERDICT: ACCEPT-MANAGED\nfn main() {}\n"
        payload = {"summary": {"verdict": "accept"},
                   "diagnostics": [{"severity": "warning", "code": "HYB-E8001"}]}
        run = self.run_corpus(source, payload)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 2/2", run.stdout)
        run = self.run_corpus(source)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("expected accept + managed lint", run.stdout)


if __name__ == "__main__":
    unittest.main()
