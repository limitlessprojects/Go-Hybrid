"""Regression checks for malformed EXPECTED-VERDICT annotations.

Parser tests exercise the real module. CLI tests use a disposable Python
checker/builder, not the Hybrid compiler, and run actual child processes.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HARNESS = Path(__file__).with_name("differential_loop.py")
SPEC = importlib.util.spec_from_file_location("differential_marker_harness", HARNESS)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class VerdictMarkerParserTests(unittest.TestCase):
    def parse(self, text):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "case.hy"
            path.write_text(text, encoding="utf-8")
            return MODULE.read_expected(path)

    def assert_invalid(self, text):
        with self.assertRaises(SystemExit) as caught:
            self.parse(text)
        self.assertIn("corpus error", str(caught.exception))

    def test_unknown_verdict_cannot_be_silently_skipped(self):
        self.assert_invalid("// EXPECTED-VERDICT: REJET A1\nfn main() {}\n")

    def test_empty_verdict_is_invalid(self):
        self.assert_invalid("// EXPECTED-VERDICT: \nfn main() {}\n")

    def test_accept_suffix_is_not_a_valid_verdict(self):
        self.assert_invalid("// EXPECTED-VERDICT: ACCEPT-BROKEN\nfn main() {}\n")

    def test_managed_suffix_is_not_a_valid_verdict(self):
        self.assert_invalid("// EXPECTED-VERDICT: ACCEPT-MANAGED-BROKEN\n")

    def test_conflicting_markers_are_not_first_match_wins(self):
        self.assert_invalid("// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-VERDICT: REJECT A1\n")

    def test_repeated_identical_markers_are_ambiguous(self):
        self.assert_invalid("// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-VERDICT: ACCEPT\n")

    def test_reject_without_rule_does_not_consume_next_source_line(self):
        self.assert_invalid("// EXPECTED-VERDICT: REJECT\nfn main() {}\n")

    def test_reject_without_rule_does_not_consume_next_comment(self):
        self.assert_invalid("// EXPECTED-VERDICT: REJECT\n// reason follows\n")

    def test_accept_does_not_capture_a_rule_from_next_line(self):
        self.assertEqual(self.parse("// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n"),
                         ("ACCEPT", None, []))

    def test_valid_reject_preserves_rule_and_inline_explanation(self):
        self.assertEqual(self.parse("// EXPECTED-VERDICT: REJECT A1b (move rule)\n"),
                         ("REJECT", "A1b", []))

    def test_valid_managed_preserves_hazard(self):
        source = "// EXPECTED-VERDICT: ACCEPT-MANAGED\n// RUNTIME-HAZARD: Concurrent Map Writes\n"
        verdict, rule, hazards = self.parse(source)
        self.assertEqual((verdict, hazards), ("ACCEPT-MANAGED", ["concurrent map writes"]))

    def test_unregistered_helper_is_still_skipped(self):
        self.assertIsNone(self.parse("// helper, no registered expected verdict\nfn helper() {}\n"))

    def test_tabs_and_crlf_are_accepted(self):
        self.assertEqual(self.parse("//\tEXPECTED-VERDICT:\tREJECT\tA1b\r\nfn main() {}\r\n"),
                         ("REJECT", "A1b", []))


@unittest.skipUnless(os.name == "posix", "CLI fixture uses POSIX executable scripts")
class VerdictMarkerCLITests(unittest.TestCase):
    def run_case(self, second_source):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-control.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8")
            (corpus / "02-case.hy").write_text(second_source, encoding="utf-8")
            fake = root / "fake-hyb"
            payload = json.dumps({"summary": {"verdict": "reject"},
                                  "diagnostics": [{"severity": "error", "tier0_rule": "A1"}]})
            executable = f"#!{sys.executable}\n"
            fake.write_text(
                f"#!{sys.executable}\nimport pathlib, sys\n"
                "if sys.argv[1] == 'check':\n"
                f"    print({payload!r})\n    sys.exit(1)\n"
                "p = pathlib.Path('a.out')\n"
                f"p.write_text({executable!r}, encoding='utf-8')\np.chmod(0o755)\n",
                encoding="utf-8")
            fake.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(fake), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10)

    def assert_malformed_corpus_fails(self, source):
        result = self.run_case(source)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("FULL AGREEMENT", result.stdout)
        self.assertIn("corpus error", result.stderr)
        self.assertIn("02-case.hy", result.stderr)

    def test_typo_does_not_disappear_beside_a_passing_case(self):
        self.assert_malformed_corpus_fails("// EXPECTED-VERDICT: REJET A1\nfn main() {}\n")

    def test_empty_marker_does_not_disappear_beside_a_passing_case(self):
        self.assert_malformed_corpus_fails("// EXPECTED-VERDICT: \nfn main() {}\n")

    def test_suffixed_accept_cannot_pass_via_prefix_matching(self):
        self.assert_malformed_corpus_fails("// EXPECTED-VERDICT: ACCEPT-BROKEN\nfn main() {}\n")

    def test_conflicting_markers_cannot_be_reported_as_agreement(self):
        self.assert_malformed_corpus_fails(
            "// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n")

    def test_valid_mixed_accept_and_reject_corpus_still_passes(self):
        result = self.run_case("// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)
        self.assertIn("reject A1", result.stdout)

    def test_unregistered_helpers_do_not_break_a_valid_corpus(self):
        result = self.run_case("// deliberately unregistered helper\nfn helper() {}\n")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)


if __name__ == "__main__":
    unittest.main()
