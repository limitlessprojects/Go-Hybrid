"""Exact diagnostic pins must survive the full differential CLI (fake checker)."""

from pathlib import Path
import json
import subprocess
import sys
import tempfile
import unittest


HARNESS = Path(__file__).with_name("differential_loop.py")


class DifferentialDiagnosticMetadataTests(unittest.TestCase):
    def run_case(self, metadata, diagnostics=None, verdict="REJECT BUILDABLE"):
        if diagnostics is None:
            diagnostics = [
                {"severity": "error", "tier0_rule": "BUILDABLE", "code": "HYB-E1002"}
            ]
        payload = {"summary": {"verdict": "reject"}, "diagnostics": diagnostics}
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-case.hy").write_text(
                f"// EXPECTED-VERDICT: {verdict}\n{metadata}\nfn main() {{}}\n",
                encoding="utf-8",
            )
            is_win = sys.platform == "win32"
            checker = root / ("hyb.cmd" if is_win else "hyb")
            if is_win:
                py_script = root / "hyb.py"
                py_script.write_text(
                    "import json, pathlib, sys\n"
                    "if len(sys.argv) > 1 and sys.argv[1] == 'check':\n"
                    f"    print(json.dumps({payload!r}))\n    raise SystemExit(1)\n"
                    "binary = pathlib.Path('a.out')\n"
                    f"binary.write_text({('#!' + sys.executable + chr(10))!r}, encoding='utf-8')\n",
                    encoding="utf-8",
                )
                checker.write_text(
                    f'@echo off\n"{sys.executable}" "{py_script}" %*\n',
                    encoding="utf-8",
                )
            else:
                checker.write_text(
                    f"#!{sys.executable}\nimport json, pathlib, sys\n"
                    "if sys.argv[1] == 'check':\n"
                    f"    print(json.dumps({payload!r}))\n    raise SystemExit(1)\n"
                    "binary = pathlib.Path('a.out')\n"
                    f"binary.write_text({('#!' + sys.executable + chr(10))!r}, encoding='utf-8')\n"
                    "binary.chmod(0o700)\n",
                    encoding="utf-8",
                )
                checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def test_matching_code_passes(self):
        result = self.run_case("// EXPECTED-DIAGNOSTIC: HYB-E1002")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_wrong_code_with_matching_rule_fails(self):
        result = self.run_case("// EXPECTED-DIAGNOSTIC: HYB-E2002")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertNotIn("FULL AGREEMENT", result.stdout)
        self.assertIn("expected first diagnostic HYB-E2002, got HYB-E1002", result.stdout)

    def test_missing_actual_code_fails(self):
        result = self.run_case(
            "// EXPECTED-DIAGNOSTIC: HYB-E1002",
            [{"severity": "error", "tier0_rule": "BUILDABLE"}],
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_later_matching_code_cannot_hide_wrong_first_error(self):
        result = self.run_case(
            "// EXPECTED-DIAGNOSTIC: HYB-E2002",
            [{"severity": "error", "tier0_rule": "BUILDABLE", "code": "HYB-E1002"},
             {"severity": "error", "tier0_rule": "BUILDABLE", "code": "HYB-E2002"}],
        )
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_warning_before_matching_error_does_not_change_pin(self):
        result = self.run_case(
            "// EXPECTED-DIAGNOSTIC: HYB-E1002",
            [{"severity": "warning", "code": "HYB-E8001"},
             {"severity": "error", "tier0_rule": "BUILDABLE", "code": "HYB-E1002"}],
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_unpinned_fixture_keeps_rule_only_contract(self):
        result = self.run_case("")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_malformed_pin_fails_as_metadata_error(self):
        for metadata in ("// EXPECTED-DIAGNOSTIC: bad",
                         "// EXPECTED-DIAGNOSTIC HYB-E1002",
                         "// EXPECTED-DIAGNOSTIC:"):
            with self.subTest(metadata=metadata):
                result = self.run_case(metadata)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn("malformed EXPECTED-DIAGNOSTIC", result.stderr)

    def test_duplicate_pin_fails_as_metadata_error(self):
        result = self.run_case(
            "// EXPECTED-DIAGNOSTIC: HYB-E1002\n// EXPECTED-DIAGNOSTIC: HYB-E1002"
        )
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("multiple EXPECTED-DIAGNOSTIC", result.stderr)

    def test_accept_fixture_cannot_declare_error_pin(self):
        result = self.run_case("// EXPECTED-DIAGNOSTIC: HYB-E1002", verdict="ACCEPT")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("without REJECT", result.stderr)

    def test_managed_accept_cannot_declare_error_pin(self):
        result = self.run_case("// EXPECTED-DIAGNOSTIC: HYB-E1002", verdict="ACCEPT-MANAGED")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("without REJECT", result.stderr)


if __name__ == "__main__":
    unittest.main()
