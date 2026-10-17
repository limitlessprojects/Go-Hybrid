"""The differential's registered stdout assertions must affect its verdict.

Mock only the build/run processes: exercise the real corpus parsing and
main loop without requiring Go, Rust, or executable script fixtures.
"""
import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


HARNESS = Path(__file__).with_name("differential_loop.py")
SPEC = importlib.util.spec_from_file_location("differential_output_harness", HARNESS)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RegisteredOutputTests(unittest.TestCase):
    def run_corpus(self, stdout, marker="// EXPECTED-OUTPUT: idle\n"):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "35-default-fallback.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\n" + marker + "fn main() {}\n",
                encoding="utf-8",
            )
            compiler = root / "hyb"
            compiler.touch()
            build = subprocess.CompletedProcess([], 0, "", "")
            run = subprocess.CompletedProcess([], 0, stdout, "")
            output = io.StringIO()
            with patch.object(sys, "argv", [
                str(HARNESS), "--bin", str(compiler), "--corpus", str(corpus)
            ]), patch.object(MODULE.subprocess, "run", side_effect=[build, run]), \
                    patch.object(MODULE.os.path, "exists", return_value=True), \
                    contextlib.redirect_stdout(output):
                code = MODULE.main()
            return code, output.getvalue()

    def test_wrong_output_with_exit_zero_is_not_full_agreement(self):
        code, output = self.run_corpus("sent\n")
        self.assertEqual(code, 1, output)
        self.assertNotIn("FULL AGREEMENT", output)
        self.assertIn("stdout", output)

    def test_matching_output_passes(self):
        code, output = self.run_corpus("idle\n")
        self.assertEqual(code, 0, output)
        self.assertIn("FULL AGREEMENT", output)

    def test_missing_output_is_a_failure(self):
        code, output = self.run_corpus("")
        self.assertEqual(code, 1, output)
        self.assertNotIn("FULL AGREEMENT", output)

    def test_extra_blank_line_is_not_hidden(self):
        code, output = self.run_corpus("idle\n\n")
        self.assertEqual(code, 1, output)

    def test_line_ending_and_optional_terminal_newline(self):
        for stdout in ("idle", "idle\r\n"):
            with self.subTest(stdout=stdout):
                code, output = self.run_corpus(stdout)
                self.assertEqual(code, 0, output)

    def test_fixture_without_output_marker_keeps_execution_contract(self):
        code, output = self.run_corpus("arbitrary stdout\n", marker="")
        self.assertEqual(code, 0, output)

    def test_duplicate_output_markers_are_invalid(self):
        with self.assertRaisesRegex(SystemExit, "multiple EXPECTED-OUTPUT"):
            self.run_corpus("idle\n", marker=(
                "// EXPECTED-OUTPUT: idle\n// EXPECTED-OUTPUT: sent\n"
            ))

    def test_malformed_output_marker_is_not_ignored(self):
        with self.assertRaisesRegex(SystemExit, "malformed EXPECTED-OUTPUT"):
            self.run_corpus("idle\n", marker="// EXPECTED-OUTPUT idle\n")


class OutputMetadataTests(unittest.TestCase):
    def test_empty_marker_is_invalid(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "case.hy"
            path.write_text("// EXPECTED-OUTPUT: \n", encoding="utf-8")
            with self.assertRaisesRegex(SystemExit, "malformed EXPECTED-OUTPUT"):
                MODULE.read_expected_output(path, ("ACCEPT", None, []))

    def test_output_assertion_requires_an_accept_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "case.hy"
            path.write_text("// EXPECTED-OUTPUT: idle\n", encoding="utf-8")
            for expected in (None, ("REJECT", "A1", [])):
                with self.subTest(expected=expected):
                    with self.assertRaisesRegex(SystemExit, "without ACCEPT"):
                        MODULE.read_expected_output(path, expected)

    def test_real_default_fallback_keeps_its_registered_output(self):
        path = HARNESS.parent.parent / "tests/checker-corpus/35-default-fallback.hy"
        expected = MODULE.read_expected(path)
        self.assertEqual(MODULE.read_expected_output(path, expected), "idle")


if __name__ == "__main__":
    unittest.main()
