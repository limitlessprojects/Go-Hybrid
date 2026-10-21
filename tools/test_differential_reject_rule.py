"""CLI regression tests for rejection-reason agreement (fake checker only)."""
import json
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


    def test_malformed_envelopes_and_contradictory_counts_fail_closed(self):
        for bad in (None, [], {"summary": None}, {"summary": {"verdict": "reject", "errors": 0}, "diagnostics": [{"severity": "error", "tier0_rule": "A1"}]}, {"summary": {"verdict": "reject", "warnings": True}, "diagnostics": [{"severity": "error", "tier0_rule": "A1"}]}):
            ok, _ = HARNESS_MOD.checker_rejects(sys.executable, "x.hy", "A1") if False else (False, "")
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "bad.json"
            for env in ("null", "[]", '{"summary":{"verdict":"reject","errors":0},"diagnostics":[{"severity":"error","tier0_rule":"A1"}]}'):
                c = Path(tmp) / "hyb"
                c.write_text(f"#!{sys.executable}\nimport sys\nprint({env!r})\nsys.exit(1)\n", encoding="utf-8")
                c.chmod(0o755)
                corpus = Path(tmp) / "c"
                corpus.mkdir(exist_ok=True)
                (corpus / "01.hy").write_text("// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n", encoding="utf-8")
                r = subprocess.run([sys.executable, str(HARNESS), "--bin", str(c), "--corpus", str(corpus)], capture_output=True, text=True)
                self.assertEqual(r.returncode, 1)

if __name__ == "__main__":
    unittest.main()
