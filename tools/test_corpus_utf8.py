"""Real-pipe regressions for checker output encoding (no Hybrid binary needed)."""

import contextlib
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


HARNESS = Path(__file__).with_name("corpus_harness.py")
SPEC = importlib.util.spec_from_file_location("corpus_utf8_harness", HARNESS)
HARNESS_MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS_MOD)
REAL_RUN = subprocess.run


class CheckerUtf8Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.checker = self.root / "checker.py"
        # Emit exact bytes, independently of the child's own locale.
        self.checker.write_text(
            "import json, sys\n"
            "case = sys.argv[2]\n"
            "if case.endswith('invalid-stdout.hy'):\n"
            "    sys.stdout.buffer.write(b'\\xff')\n"
            "elif case.endswith('error.hy'):\n"
            "    sys.stderr.buffer.write('checker: \\u9519'.encode('utf-8'))\n"
            "    raise SystemExit(2)\n"
            "else:\n"
            "    value = {'summary': {'verdict': 'accept', 'errors': 0, 'warnings': 0},\n"
            "             'diagnostics': [{'severity': 'note', 'message': '\\u9519'}]}\n"
            "    if case.endswith('ascii.hy'):\n"
            "        value['diagnostics'][0]['message'] = 'plain note'\n"
            "    sys.stdout.buffer.write(json.dumps(value, ensure_ascii=False).encode('utf-8'))\n"
            "    if case.endswith('invalid-stderr.hy'):\n"
            "        sys.stderr.buffer.write(b'\\xff')\n",
            encoding="utf-8",
        )

    def run_with_legacy_locale(self, command, **kwargs):
        # Keep real OS pipes/decoding. Select the legacy fallback on every host
        # so Linux CI also exercises the Windows cp936 failure, without changing
        # global locale or mocking the checker's output/CompletedProcess.
        if kwargs.get("text") or kwargs.get("universal_newlines"):
            kwargs.setdefault("encoding", "cp936")
        return REAL_RUN(
            [sys.executable, str(self.checker), *command[1:]], **kwargs
        )

    def run_case(self, name):
        with patch.object(HARNESS_MOD.subprocess, "run", self.run_with_legacy_locale):
            return HARNESS_MOD.run_checker(str(self.checker), name)

    def test_utf8_note_survives_non_utf8_locale(self):
        summary, diagnostics = self.run_case("valid.hy")
        self.assertEqual(summary["verdict"], "accept")
        self.assertEqual(diagnostics[0]["message"], "\u9519")

    def test_ascii_output_keeps_the_existing_contract(self):
        summary, diagnostics = self.run_case("ascii.hy")
        self.assertEqual(summary["verdict"], "accept")
        self.assertEqual(diagnostics[0]["message"], "plain note")

    def test_utf8_stderr_preserves_checker_failure_detail(self):
        with self.assertRaisesRegex(RuntimeError, "checker: \u9519"):
            self.run_case("error.hy")

    def test_invalid_utf8_stdout_is_a_reportable_failure(self):
        with self.assertRaisesRegex(RuntimeError, "UTF-8"):
            self.run_case("invalid-stdout.hy")

    def test_invalid_utf8_stderr_is_a_reportable_failure(self):
        with self.assertRaisesRegex(RuntimeError, "UTF-8"):
            self.run_case("invalid-stderr.hy")

    def test_invalid_output_does_not_skip_the_next_fixture(self):
        corpus = self.root / "corpus"
        corpus.mkdir()
        for name in ("01-invalid-stdout.hy", "02-valid.hy"):
            (corpus / name).write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
            )
        output = io.StringIO()
        with patch.object(sys, "argv", [
            str(HARNESS), "--bin", str(self.checker), "--corpus", str(corpus)
        ]), patch.object(HARNESS_MOD.subprocess, "run", self.run_with_legacy_locale), \
                contextlib.redirect_stdout(output):
            status = HARNESS_MOD.main()
        self.assertEqual(status, 1)
        text = output.getvalue()
        self.assertIn("harness-error", text)
        self.assertIn("02-valid.hy", text)
        self.assertIn("verdict agreement: 1/2", text)


if __name__ == "__main__":
    unittest.main()
