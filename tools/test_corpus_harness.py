"""CLI regressions for optional exact diagnostic metadata (stdlib only)."""

from pathlib import Path
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

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
            checker = root / "check"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit(1)\n",
                encoding="utf-8",
            )
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", sys.executable, "--corpus", str(corpus)],
                cwd=root, env={**os.environ, "PYTHONIOENCODING": "utf-8"},
                capture_output=True, text=True, encoding="utf-8", timeout=10,
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
            checker = root / "check"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit({status})\n",
                encoding="utf-8",
            )
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", sys.executable, "--corpus", str(corpus)],
                cwd=root, env={**os.environ, "PYTHONIOENCODING": "utf-8"},
                capture_output=True, text=True, encoding="utf-8", timeout=10,
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


class CheckerTextEncodingTests(unittest.TestCase):
    """Real UTF-8 child output must not depend on the host's text encoding."""

    def run_payload(self, payload, status=0, stderr=""):
        spec = importlib.util.spec_from_file_location("encoding_harness", HARNESS)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        script = (
            "import sys\n"
            f"sys.stdout.buffer.write({json.dumps(payload, ensure_ascii=False).encode('utf-8')!r})\n"
            f"sys.stderr.buffer.write({stderr.encode('utf-8')!r})\n"
            f"raise SystemExit({status})\n"
        )
        real_run = subprocess.run

        def legacy_locale_run(command, **kwargs):
            # Simulate a Windows CP1252 default on every test host. Keep the
            # actual subprocess and pipe decoding; only replace the compiler
            # with a deterministic UTF-8 producer. Explicit encodings win.
            kwargs.setdefault("encoding", "cp1252")
            return real_run([sys.executable, "-c", script], **kwargs)

        with patch.object(module.subprocess, "run", side_effect=legacy_locale_run):
            return module.run_checker("fake-hyb", "case.hy")

    def test_non_ascii_diagnostic_message_is_preserved(self):
        payload = {
            "summary": {"verdict": "accept", "warnings": 1},
            "diagnostics": [{"severity": "warning", "message": "archivo caf\u00e9"}],
        }
        summary, diagnostics = self.run_payload(payload)
        self.assertEqual(summary, payload["summary"])
        self.assertEqual(diagnostics, payload["diagnostics"])

    def test_utf8_outside_cp1252_does_not_crash(self):
        payload = {
            "summary": {"verdict": "accept", "warnings": 1},
            "diagnostics": [{"severity": "warning", "message": "variable \u0401"}],
        }
        self.assertEqual(self.run_payload(payload)[1], payload["diagnostics"])

    def test_rejection_preserves_unicode_source_path(self):
        payload = {
            "summary": {"verdict": "reject", "errors": 1},
            "diagnostics": [{"severity": "error", "tier0_rule": "A1c",
                             "span": {"file": "examples/ni\u00f1o.hy"}}],
        }
        summary, diagnostics = self.run_payload(payload, status=1)
        self.assertEqual(summary, payload["summary"])
        self.assertEqual(diagnostics, payload["diagnostics"])

    def test_unexpected_exit_keeps_utf8_stderr(self):
        with self.assertRaisesRegex(RuntimeError, "checker exited unexpectedly.*fallo \u0401"):
            self.run_payload({"summary": {"verdict": "accept"}, "diagnostics": []},
                             status=2, stderr="fallo \u0401")

    def test_ascii_control_is_unchanged(self):
        payload = {"summary": {"verdict": "accept"}, "diagnostics": []}
        self.assertEqual(self.run_payload(payload), (payload["summary"], []))


if __name__ == "__main__":
    unittest.main()
