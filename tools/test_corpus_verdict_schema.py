"""Real-pipe regressions for checker verdict schema and error handling (#2930)."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HARNESS = Path(__file__).with_name("corpus_harness.py")
SPEC = importlib.util.spec_from_file_location("corpus_verdict_harness", HARNESS)
HARNESS_MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS_MOD)
REAL_RUN = subprocess.run


class CheckerVerdictSchemaTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.checker = self.root / "checker.py"
        self.checker.write_text(
            "import json, sys\nfrom pathlib import Path\n"
            "case = json.loads(Path(sys.argv[2]).with_suffix('.json').read_text(encoding='utf-8'))\n"
            "print(json.dumps(case['payload']))\nraise SystemExit(case['status'])\n",
            encoding="utf-8",
        )

    def run_python_checker(self, command, **kwargs):
        # Keep real processes, pipes and JSON decoding on Windows and POSIX.
        # Only the executable dispatch changes, like test_corpus_utf8.py.
        return REAL_RUN([sys.executable, str(self.checker), *command[1:]], **kwargs)

    def make_case(self, name, payload, expected="ACCEPT", status=0):
        fixture = self.root / name
        fixture.write_text(
            f"// EXPECTED-VERDICT: {expected}\nfn main() {{}}\n", encoding="utf-8"
        )
        fixture.with_suffix(".json").write_text(
            json.dumps({"payload": payload, "status": status}), encoding="utf-8"
        )
        return fixture

    def run_payload(self, payload, status=0):
        fixture = self.make_case("case.hy", payload, status=status)
        with patch.object(HARNESS_MOD.subprocess, "run", self.run_python_checker):
            return HARNESS_MOD.run_checker(str(self.checker), str(fixture))

    def assert_invalid_verdict(self, summary):
        # Check the exception contract, including the former TypeError cases.
        # A wrong exception is an assertion failure with its concrete type.
        try:
            self.run_payload({"summary": summary, "diagnostics": []})
        except Exception as exc:
            self.assertIsInstance(exc, RuntimeError)
            self.assertIn("verdict", str(exc))
        else:
            self.fail("invalid verdict did not raise a per-case RuntimeError")

    def test_container_verdicts_are_reportable_errors(self):
        for verdict in ([], {}, ["accept"], {"accept": 0}):
            with self.subTest(verdict=verdict):
                self.assert_invalid_verdict({"verdict": verdict})

    def test_unsupported_scalar_verdicts_are_reportable_errors(self):
        for verdict in (None, False, True, 0, 17, 0.5, "", "ACCEPT", "unknown"):
            with self.subTest(verdict=verdict):
                self.assert_invalid_verdict({"verdict": verdict})

    def test_missing_verdict_is_a_reportable_error(self):
        self.assert_invalid_verdict({})

    def test_accept_and_reject_keep_optional_counts_contract(self):
        for verdict, status in (("accept", 0), ("reject", 1)):
            with self.subTest(verdict=verdict):
                summary = {"verdict": verdict}
                payload = {"summary": summary, "diagnostics": []}
                self.assertEqual(self.run_payload(payload, status), (summary, []))

    def test_valid_verdict_still_requires_matching_exit(self):
        for verdict, status in (("accept", 1), ("reject", 0)):
            with self.subTest(verdict=verdict):
                with self.assertRaisesRegex(RuntimeError, "exit/verdict mismatch"):
                    self.run_payload({"summary": {"verdict": verdict}, "diagnostics": []}, status)

    def test_bad_verdict_does_not_skip_next_corpus_fixture(self):
        for verdict in ([], {}, None, "unknown"):
            with self.subTest(verdict=verdict):
                self.make_case("01-bad.hy", {"summary": {"verdict": verdict}, "diagnostics": []})
                self.make_case("02-valid.hy", {"summary": {"verdict": "accept"}, "diagnostics": []})
                output = io.StringIO()
                with patch.object(sys, "argv", [
                    "corpus_harness.py", "--bin", str(self.checker), "--corpus", str(self.root)
                ]), patch.object(HARNESS_MOD.subprocess, "run", self.run_python_checker), \
                        contextlib.redirect_stdout(output):
                    status = HARNESS_MOD.main()
                self.assertEqual(status, 1)
                self.assertIn("harness-error", output.getvalue())
                self.assertIn("02-valid.hy", output.getvalue())
                self.assertIn("verdict agreement: 1/2", output.getvalue())


if __name__ == "__main__":
    unittest.main()
