"""Edge-case regression tests for tools/differential_loop.py:
1. EXPECTED-VERDICT header validation (missing colon, trailing tokens on ACCEPT,
   parenthesized note without rule ID on REJECT, extra unparenthesized tokens).
2. EXPECTED-DIAGNOSTIC header validation and first-error code enforcement on
   REJECT fixtures.
3. OSError handling and UTF-8 / bytes stream decoding in build_and_run.
"""
import contextlib
import importlib.util
import io
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch


HARNESS = pathlib.Path(__file__).resolve().parent.parent / "differential_loop.py"
SPEC = importlib.util.spec_from_file_location("differential_edge_harness", HARNESS)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)


class DifferentialLoopEdgeTests(unittest.TestCase):
    def parse_verdict(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "case.hy"
            path.write_text(text, encoding="utf-8")
            return MOD.read_expected(path)

    def parse_diag(self, text):
        with tempfile.TemporaryDirectory() as tmp:
            path = pathlib.Path(tmp) / "case.hy"
            path.write_text(text, encoding="utf-8")
            expected = MOD.read_expected(path)
            return MOD.read_expected_diagnostic(path, expected)

    def test_verdict_header_without_colon_or_with_extra_tokens_is_rejected(self):
        bad_cases = [
            "// EXPECTED-VERDICT ACCEPT\nfn main() {}\n",
            "// EXPECTED-VERDICT REJECT A1\nfn main() {}\n",
            "// EXPECTED-VERDICT: ACCEPT A1\nfn main() {}\n",
            "// EXPECTED-VERDICT: ACCEPT-MANAGED extra\nfn main() {}\n",
            "// EXPECTED-VERDICT: REJECT (note without rule)\nfn main() {}\n",
            "// EXPECTED-VERDICT: REJECT A1 extra\nfn main() {}\n",
        ]
        for src in bad_cases:
            with self.subTest(src=src):
                with self.assertRaises(SystemExit) as ctx:
                    self.parse_verdict(src)
                self.assertIn("corpus error", str(ctx.exception))

    def test_expected_diagnostic_header_parsing_and_validation(self):
        code = self.parse_diag(
            "// EXPECTED-VERDICT: REJECT A1\n"
            "// EXPECTED-DIAGNOSTIC: HYB-E1001\n"
            "fn main() {}\n"
        )
        self.assertEqual(code, "HYB-E1001")

        invalid_diag_cases = [
            "// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-DIAGNOSTIC: HYB-E1001\n",
            "// EXPECTED-VERDICT: REJECT A1\n// EXPECTED-DIAGNOSTIC HYB-E1001\n",
            "// EXPECTED-VERDICT: REJECT A1\n// EXPECTED-DIAGNOSTIC: BAD-CODE\n",
            (
                "// EXPECTED-VERDICT: REJECT A1\n"
                "// EXPECTED-DIAGNOSTIC: HYB-E1001\n"
                "// EXPECTED-DIAGNOSTIC: HYB-E1002\n"
            ),
        ]
        for src in invalid_diag_cases:
            with self.subTest(src=src):
                with self.assertRaises(SystemExit) as ctx:
                    self.parse_diag(src)
                self.assertIn("corpus error", str(ctx.exception))

    def test_checker_rejects_enforces_expected_diagnostic_code(self):
        payload = json.dumps({
            "summary": {"verdict": "reject", "errors": 1},
            "diagnostics": [
                {"severity": "error", "tier0_rule": "A1", "code": "HYB-E1002"}
            ],
        }).encode("utf-8")
        proc = subprocess.CompletedProcess([], 1, payload, b"")
        with patch.object(MOD.subprocess, "run", return_value=proc):
            ok_match, _ = MOD.checker_rejects(
                "hyb", "case.hy", "A1", expected_diag="HYB-E1002"
            )
            ok_mismatch, detail = MOD.checker_rejects(
                "hyb", "case.hy", "A1", expected_diag="HYB-E1001"
            )
        self.assertTrue(ok_match)
        self.assertFalse(ok_mismatch)
        self.assertIn("expected_code=HYB-E1001", detail)

    def test_build_and_run_handles_oserror_and_bytes_streams(self):
        with patch.object(MOD.subprocess, "run", side_effect=OSError("exec failed")):
            ok, detail = MOD.build_and_run("hyb", "case.hy", ".")
        self.assertFalse(ok)
        self.assertIn("build could not be run", detail)

        build_ok = subprocess.CompletedProcess([], 0, b"", b"")
        with patch.object(MOD.subprocess, "run", side_effect=[build_ok, OSError("bad bin")]), \
             patch.object(MOD.os.path, "exists", return_value=True):
            ok, detail = MOD.build_and_run("hyb", "case.hy", ".")
        self.assertFalse(ok)
        self.assertIn("run could not be started", detail)

        run_utf8 = subprocess.CompletedProcess([], 0, "héllo\n".encode("utf-8"), b"")
        with patch.object(MOD.subprocess, "run", side_effect=[build_ok, run_utf8]), \
             patch.object(MOD.os.path, "exists", return_value=True):
            ok, detail = MOD.build_and_run(
                "hyb", "case.hy", ".", expected_output="héllo"
            )
        self.assertTrue(ok, detail)
