"""CLI regression tests for rejection-reason agreement (fake checker only)."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HARNESS = Path(__file__).with_name("differential_loop.py")


class RejectRuleTests(unittest.TestCase):
    def run_case(self, diagnostics):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-accept.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8")
            (corpus / "02-reject.hy").write_text(
                "// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n", encoding="utf-8")
            payload = json.dumps({"summary": {"verdict": "reject"},
                                  "diagnostics": diagnostics})
            program = f"#!{sys.executable}\n"
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport pathlib, sys\n"
                "if sys.argv[1] == 'check':\n"
                f"    print({payload!r})\n    sys.exit(1)\n"
                "p = pathlib.Path('a.out')\n"
                f"p.write_text({program!r}, encoding='utf-8')\np.chmod(0o755)\n",
                encoding="utf-8")
            checker.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker),
                 "--corpus", str(corpus)], capture_output=True, text=True,
                 timeout=10)

    def assert_result(self, diagnostics, expected_code):
        result = self.run_case(diagnostics)
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertEqual("FULL AGREEMENT" in result.stdout, expected_code == 0)
        return result

    def test_matching_first_error_passes(self):
        self.assert_result([{"severity": "error", "tier0_rule": "A1"}], 0)

    def test_wrong_rule_fails(self):
        result = self.assert_result([{"severity": "error", "tier0_rule": "FC"}], 1)
        self.assertIn("first_rule=FC", result.stdout)

    def test_unclassified_error_fails(self):
        self.assert_result([{"severity": "error", "code": "HYB-E0002"}], 1)

    def test_later_matching_error_cannot_hide_wrong_first_error(self):
        self.assert_result([{"severity": "error", "tier0_rule": "FC"},
                            {"severity": "error", "tier0_rule": "A1"}], 1)

    def test_warning_does_not_override_matching_first_error(self):
        self.assert_result([{"severity": "warning", "tier0_rule": "FC"},
                            {"severity": "error", "tier0_rule": "A1"}], 0)

    def test_matching_warning_without_errors_fails(self):
        self.assert_result([{"severity": "warning", "tier0_rule": "A1"}], 1)


class RejectEnvelopeTests(unittest.TestCase):
    def run_raw_payload(self, raw_payload, exit_code=1):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-accept.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8")
            (corpus / "02-reject.hy").write_text(
                "// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n", encoding="utf-8")
            program = f"#!{sys.executable}\n"
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport pathlib, sys\n"
                "if sys.argv[1] == 'check':\n"
                f"    print({raw_payload!r})\n    sys.exit({exit_code})\n"
                "p = pathlib.Path('a.out')\n"
                f"p.write_text({program!r}, encoding='utf-8')\np.chmod(0o755)\n",
                encoding="utf-8")
            checker.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker),
                 "--corpus", str(corpus)], capture_output=True, text=True,
                timeout=10)

    def assert_invalid(self, raw_payload, exit_code=1):
        res = self.run_raw_payload(raw_payload, exit_code=exit_code)
        self.assertEqual(res.returncode, 1, res.stdout + res.stderr)
        self.assertNotIn("Traceback", res.stderr)
        self.assertIn("1 disagreement", res.stdout)
        self.assertIn("FAIL  02-reject.hy", res.stdout)

    def test_list_payload_fails_without_traceback(self):
        self.assert_invalid("[]")

    def test_null_payload_fails_without_traceback(self):
        self.assert_invalid("null")

    def test_string_payload_fails_without_traceback(self):
        self.assert_invalid('"hello"')

    def test_non_dict_summary_fails_without_traceback(self):
        self.assert_invalid('{"summary": "not-a-dict", "diagnostics": []}')

    def test_non_list_diagnostics_fails_without_traceback(self):
        self.assert_invalid('{"summary": {"verdict": "reject"}, "diagnostics": "not-a-list"}')

    def test_non_dict_diagnostic_item_fails_without_traceback(self):
        self.assert_invalid('{"summary": {"verdict": "reject"}, "diagnostics": [null]}')

    def test_contradictory_error_count_fails_without_traceback(self):
        self.assert_invalid('{"summary": {"verdict": "reject", "errors": 0}, "diagnostics": [{"severity": "error", "tier0_rule": "A1"}]}')

    def test_contradictory_summary_count_variants_fail(self):
        # from pr-601 (residual of #582/#260): both count fields must be
        # nonnegative ints matching their diagnostic entries; booleans,
        # strings, and nulls are not counts.
        diag = '[{"severity": "error", "tier0_rule": "A1"}]'
        for field, count in (("errors", True), ("errors", -1), ("errors", '"1"'),
                             ("errors", "null"), ("warnings", 5), ("warnings", True),
                             ("warnings", -1)):
            with self.subTest(field=field, count=count):
                summary = '{"verdict": "reject", "errors": 1, "warnings": 0, "%s": %s}' % (field, count)
                self.assert_invalid('{"summary": %s, "diagnostics": %s}' % (summary, diag))

    def test_contradictory_warning_count_fails_without_traceback(self):
        self.assert_invalid(
            '{"summary": {"verdict": "reject", "errors": 1, "warnings": 5}, '
            '"diagnostics": [{"severity": "error", "tier0_rule": "A1"}]}')

    def test_consistent_error_and_warning_counts_preserve_agreement(self):
        # from pr-601: older fixtures omitting counts, and consistent
        # counts, both keep working.
        res = self.run_raw_payload(
            '{"summary": {"verdict": "reject", "errors": 1, "warnings": 1}, '
            '"diagnostics": [{"severity": "error", "tier0_rule": "A1"}, '
            '{"severity": "warning", "code": "HYB-E8001"}]}', exit_code=1)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertIn("FULL AGREEMENT", res.stdout)

    def test_omitted_counts_still_agree(self):
        res = self.run_raw_payload(
            '{"summary": {"verdict": "reject"}, '
            '"diagnostics": [{"severity": "error", "tier0_rule": "A1"}]}', exit_code=1)
        self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
        self.assertIn("FULL AGREEMENT", res.stdout)

    def test_empty_stdout_fails_without_traceback(self):
        self.assert_invalid("")


def fake_checker(root, raw_payload):
    """An executable fake `hyb` that answers `check` with `raw_payload` (exit 1).

    POSIX runs a shebang script; Windows cannot exec one, so it gets a .cmd
    shim around the same Python source. The reject-side cases never build or
    run a program, so this keeps them runnable on both platforms.
    """
    script = ("import sys\n"
              "if sys.argv[1] == 'check':\n"
              f"    print({raw_payload!r})\n"
              "    sys.exit(1)\n")
    if os.name == "nt":
        helper = root / "fake_hyb.py"
        helper.write_text(script, encoding="utf-8")
        checker = root / "hyb.cmd"
        checker.write_text(
            '@echo off\n"{}" "{}" %*\n'.format(sys.executable, helper),
            encoding="utf-8")
        return checker
    checker = root / "hyb"
    checker.write_text(f"#!{sys.executable}\n" + script, encoding="utf-8")
    checker.chmod(0o755)
    return checker


class RejectDiagnosticPinTests(unittest.TestCase):
    """The optional EXPECTED-DIAGNOSTIC pin must bind the reject side too.

    Issue #2161: corpus_harness enforces the pin, the differential loop read
    only the rule, so a fixture whose code drifted reported FULL AGREEMENT.

    These cases need only the reject side, so their corpus carries a single
    REJECT fixture: no binary has to be built or executed.
    """

    PINNED = ("// EXPECTED-VERDICT: REJECT A1\n"
              "// EXPECTED-DIAGNOSTIC: HYB-E2002\nfn main() {}\n")
    UNPINNED = "// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n"

    def run_case(self, reject_source, raw_payload, accept_source=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            if accept_source is not None:
                (corpus / "01-accept.hy").write_text(accept_source, encoding="utf-8")
            (corpus / "02-reject.hy").write_text(reject_source, encoding="utf-8")
            checker = fake_checker(root, raw_payload)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker),
                 "--corpus", str(corpus)], capture_output=True, text=True,
                timeout=10)

    @staticmethod
    def checker_payload(*diagnostics):
        return json.dumps({"summary": {"verdict": "reject"},
                           "diagnostics": list(diagnostics)})

    def test_matching_pinned_code_agrees(self):
        result = self.run_case(
            self.PINNED,
            self.checker_payload({"severity": "error", "tier0_rule": "A1",
                                  "code": "HYB-E2002"}))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_drifted_code_with_matching_rule_disagrees(self):
        # The reported repro: same rule, different code, previously a pass.
        result = self.run_case(
            self.PINNED,
            self.checker_payload({"severity": "error", "tier0_rule": "A1",
                                  "code": "HYB-E1002"}))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertNotIn("FULL AGREEMENT", result.stdout)
        self.assertIn("expected first diagnostic HYB-E2002, got HYB-E1002",
                      result.stdout)
        self.assertIn("FAIL  02-reject.hy", result.stdout)

    def test_missing_code_with_pin_disagrees(self):
        result = self.run_case(
            self.PINNED, self.checker_payload({"severity": "error", "tier0_rule": "A1"}))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("expected_code=HYB-E2002", result.stdout)

    def test_unpinned_fixture_keeps_the_rule_only_contract(self):
        result = self.run_case(
            self.UNPINNED,
            self.checker_payload({"severity": "error", "tier0_rule": "A1",
                                  "code": "HYB-E1002"}))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_duplicate_pin_is_a_corpus_error(self):
        result = self.run_case(
            "// EXPECTED-VERDICT: REJECT A1\n"
            "// EXPECTED-DIAGNOSTIC: HYB-E2002\n"
            "// EXPECTED-DIAGNOSTIC: HYB-E2002\nfn main() {}\n",
            self.checker_payload())
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("multiple EXPECTED-DIAGNOSTIC", result.stderr)

    def test_malformed_pin_is_a_corpus_error(self):
        result = self.run_case(
            "// EXPECTED-VERDICT: REJECT A1\n"
            "// EXPECTED-DIAGNOSTIC: HYB-E2\nfn main() {}\n",
            self.checker_payload())
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("malformed EXPECTED-DIAGNOSTIC", result.stderr)

    def test_pin_on_an_accept_fixture_is_a_corpus_error(self):
        result = self.run_case(
            self.UNPINNED, self.checker_payload(),
            accept_source="// EXPECTED-VERDICT: ACCEPT\n"
                          "// EXPECTED-DIAGNOSTIC: HYB-E2002\nfn main() {}\n")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("without REJECT", result.stderr)



if __name__ == "__main__":
    unittest.main()
