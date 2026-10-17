"""Offline CLI regressions for the differential harness, using a fake builder.

These exercise harness decisions, not Hybrid compiler correctness.
Run: python3 -m unittest discover -s tools -p test_differential_loop.py -v
"""
import contextlib
import importlib.util
import io
import json
import os
import pathlib
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


HARNESS = pathlib.Path(__file__).with_name("differential_loop.py")
HARNESS_SPEC = importlib.util.spec_from_file_location("differential_harness", HARNESS)
HARNESS_MOD = importlib.util.module_from_spec(HARNESS_SPEC)
HARNESS_SPEC.loader.exec_module(HARNESS_MOD)


class RuntimeHazardTests(unittest.TestCase):
    def run_case(self, stderr="", stdout="", exit_code=2,
                 hazard="concurrent map writes (Go fatal, pre-registered)"):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            source = "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n"
            if hazard is not None:
                source += f"// RUNTIME-HAZARD: {hazard}\n"
            (corpus / "01-case.hy").write_text(source, encoding="utf-8")
            program = (f"#!{sys.executable}\nimport sys\n"
                       f"sys.stderr.write({stderr!r})\n"
                       f"sys.stdout.write({stdout!r})\n"
                       f"sys.exit({exit_code})\n")
            builder = root / "hyb"
            builder.write_text(
                f"#!{sys.executable}\nimport pathlib\n"
                f"p = pathlib.Path('a.out')\np.write_text({program!r})\n"
                "p.chmod(0o755)\n", encoding="utf-8")
            builder.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(builder),
                 "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def assert_result(self, result, code):
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        self.assertEqual("FULL AGREEMENT" in result.stdout, code == 0)

    def test_registered_fatal_is_allowed(self):
        self.assert_result(self.run_case(stderr="fatal error: concurrent map writes\n"), 0)

    def test_registered_fatal_before_long_stack_is_allowed(self):
        self.assert_result(self.run_case(
            stderr="fatal error: concurrent map writes\n" + "stack frame\n" * 100), 0)

    def test_unrelated_panic_fails(self):
        self.assert_result(self.run_case(stderr="panic: runtime error: index out of range\n"), 1)

    def test_unrelated_fatal_fails(self):
        self.assert_result(self.run_case(stderr="fatal error: all goroutines are asleep - deadlock!\n"), 1)

    def test_hazard_words_in_stdout_do_not_allow_unrelated_failure(self):
        self.assert_result(self.run_case(
            stdout="concurrent map writes", stderr="panic: unexpected nil pointer\n"), 1)

    def test_unregistered_fatal_fails(self):
        self.assert_result(self.run_case(
            stderr="fatal error: concurrent map writes\n", hazard=None), 1)

    def test_empty_normalized_hazard_does_not_match(self):
        self.assert_result(self.run_case(
            stderr="panic: unexpected nil pointer\n", hazard="(explanation only)"), 1)

    def test_successful_program_is_allowed(self):
        self.assert_result(self.run_case(exit_code=0), 0)

    @unittest.skipUnless(os.name == "posix", "requires POSIX abort status")
    def test_registered_fatal_abort_mode_remains_allowed(self):
        build = subprocess.CompletedProcess([], 0, "", "")
        run = subprocess.CompletedProcess(
            [], -signal.SIGABRT, "", "fatal error: concurrent map writes\n")
        with patch.object(HARNESS_MOD.subprocess, "run", side_effect=[build, run]), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            ok, detail = HARNESS_MOD.build_and_run(
                "hyb", "case.hy", ".", ["concurrent map writes"])
        self.assertTrue(ok, detail)

    def test_matching_fatal_with_exit_one_is_not_agreement(self):
        build = subprocess.CompletedProcess([], 0, "", "")
        run = subprocess.CompletedProcess(
            [], 1, "", "fatal error: concurrent map writes\n")
        with patch.object(HARNESS_MOD.subprocess, "run", side_effect=[build, run]), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            ok, detail = HARNESS_MOD.build_and_run(
                "hyb", "case.hy", ".", ["concurrent map writes"])
        self.assertFalse(ok, detail)
        self.assertIn("run exit 1", detail)

    def test_matching_fatal_with_exit_137_is_not_agreement(self):
        build = subprocess.CompletedProcess([], 0, "", "")
        run = subprocess.CompletedProcess(
            [], 137, "", "fatal error: concurrent map writes\n")
        with patch.object(HARNESS_MOD.subprocess, "run", side_effect=[build, run]), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            ok, detail = HARNESS_MOD.build_and_run(
                "hyb", "case.hy", ".", ["concurrent map writes"])
        self.assertFalse(ok, detail)
        self.assertIn("run exit 137", detail)


class CorpusCompositionTests(unittest.TestCase):
    """A registered reject-only subset is useful for focused regression runs.

    These run the actual harness CLI with a deterministic fake checker;
    they do not claim to validate the compiler.
    """

    def run_corpus(self, verdicts, actual_rule="A1"):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            corpus = root / "corpus"
            corpus.mkdir()
            for index, verdict in enumerate(verdicts):
                header = f"// EXPECTED-VERDICT: {verdict}\n" if verdict else ""
                (corpus / f"{index:02}-case.hy").write_text(
                    header + "fn main() {}\n", encoding="utf-8")
            payload = json.dumps({"summary": {"verdict": "reject"},
                                  "diagnostics": [{"severity": "error",
                                                   "tier0_rule": actual_rule}]})
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport pathlib, sys\n"
                "if sys.argv[1] == 'check':\n"
                f"    print({payload!r})\n    sys.exit(1)\n"
                "out = pathlib.Path('a.out')\n"
                f"out.write_text('#!{sys.executable}\\n', encoding='utf-8')\n"
                "out.chmod(0o755)\n", encoding="utf-8")
            checker.chmod(0o755)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker),
                 "--corpus", str(corpus)], capture_output=True, text=True,
                timeout=10)

    def test_matching_reject_only_corpus_is_full_agreement(self):
        result = self.run_corpus(["REJECT A1", "REJECT A1"])
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("differential reject-side (checker):   2/2", result.stdout)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_mismatching_reject_only_corpus_still_fails(self):
        result = self.run_corpus(["REJECT A1"], actual_rule="FC")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("first_rule=FC expected_rule=A1", result.stdout)
        self.assertNotIn("FULL AGREEMENT", result.stdout)

    def test_all_unregistered_files_are_misuse_not_agreement(self):
        result = self.run_corpus([None])
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("no registered EXPECTED-VERDICT cases", result.stderr)
        self.assertNotIn("FULL AGREEMENT", result.stdout)

    def test_mixed_corpus_still_requires_both_sides_to_agree(self):
        for rule, expected_code in (("A1", 0), ("FC", 1)):
            with self.subTest(rule=rule):
                result = self.run_corpus(["ACCEPT", "REJECT A1"], actual_rule=rule)
                self.assertEqual(result.returncode, expected_code,
                                 result.stdout + result.stderr)
                self.assertEqual("FULL AGREEMENT" in result.stdout, expected_code == 0)


class RelativeCorpusPathTests(unittest.TestCase):
    """Accept-side builds chdir into a temp workspace; relative --corpus
    paths must be resolved against the caller's cwd before that happens."""

    def _write_fake_builder(self, builder: pathlib.Path):
        # Opens argv[2] relative to the subprocess cwd (same contract as hyb).
        builder.write_text(
            f"#!{sys.executable}\n"
            "import pathlib, sys\n"
            "src = pathlib.Path(sys.argv[2])\n"
            "if not src.is_file():\n"
            "    sys.stderr.write(f'missing source: {src}')\n"
            "    sys.exit(1)\n"
            "out = pathlib.Path('a.out')\n"
            f"out.write_text('#!{sys.executable}\\n')\n"
            "out.chmod(0o755)\n",
            encoding="utf-8",
        )
        builder.chmod(0o755)

    def _run(self, *, relative_corpus: bool, relative_bin: bool = False):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary).resolve()
            corpus = root / "corpus-dir"
            corpus.mkdir()
            (corpus / "01-ok.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
            )
            builder = root / "hyb"
            self._write_fake_builder(builder)
            bin_arg = builder.name if relative_bin else str(builder)
            corpus_arg = corpus.name if relative_corpus else str(corpus)
            previous = pathlib.Path.cwd()
            try:
                os.chdir(root)
                return subprocess.run(
                    [sys.executable, str(HARNESS), "--bin", bin_arg,
                     "--corpus", corpus_arg],
                    capture_output=True, text=True, timeout=10,
                )
            finally:
                os.chdir(previous)

    def test_relative_corpus_survives_build_chdir(self):
        result = self._run(relative_corpus=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_absolute_corpus_still_works(self):
        result = self._run(relative_corpus=False)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)

    def test_relative_bin_and_corpus_together(self):
        result = self._run(relative_corpus=True, relative_bin=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("FULL AGREEMENT", result.stdout)


class TimeoutCorpusTests(unittest.TestCase):
    def run_with_timeout(self, timeout_stage):
        """The first case times out; the second case must still be evaluated."""
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            first_verdict = "REJECT A1" if timeout_stage == "checker" else "ACCEPT"
            (root / "01-timeout.hy").write_text(
                f"// EXPECTED-VERDICT: {first_verdict}\nfn main() {{}}\n",
                encoding="utf-8")
            (root / "02-ok.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8")
            first_run_pending = False

            def simulate_process(command, **kwargs):
                nonlocal first_run_pending
                if len(command) > 1 and command[1] == "check":
                    raise subprocess.TimeoutExpired(command, kwargs["timeout"])
                if len(command) > 1 and command[1] == "build":
                    is_first = pathlib.Path(command[2]).name == "01-timeout.hy"
                    if is_first and timeout_stage == "build":
                        raise subprocess.TimeoutExpired(command, kwargs["timeout"])
                    (pathlib.Path(kwargs["cwd"]) / "a.out").touch()
                    first_run_pending = is_first
                    return subprocess.CompletedProcess(command, 0, "", "")
                if first_run_pending and timeout_stage == "run":
                    first_run_pending = False
                    raise subprocess.TimeoutExpired(command, kwargs["timeout"])
                return subprocess.CompletedProcess(command, 0, "", "")

            output = io.StringIO()
            with patch.object(sys, "argv", [
                    str(HARNESS), "--bin", sys.executable, "--corpus", str(root)]), \
                 patch.object(HARNESS_MOD.subprocess, "run", side_effect=simulate_process), \
                 contextlib.redirect_stdout(output):
                status = HARNESS_MOD.main()
            return status, output.getvalue()

    def assert_timeout_is_reported_and_corpus_continues(self, stage, seconds):
        status, output = self.run_with_timeout(stage)
        self.assertEqual(status, 1, output)
        self.assertIn("FAIL  01-timeout.hy", output)
        self.assertIn(f"{stage} timed out after {seconds}s", output)
        self.assertIn("PASS  02-ok.hy", output)
        self.assertIn("differential: 1 disagreement(s)", output)
        self.assertNotIn("FULL AGREEMENT", output)

    def test_checker_timeout_reports_failure_and_continues(self):
        self.assert_timeout_is_reported_and_corpus_continues("checker", 60)

    def test_build_timeout_reports_failure_and_continues(self):
        self.assert_timeout_is_reported_and_corpus_continues("build", 300)

    def test_program_timeout_reports_failure_and_continues(self):
        self.assert_timeout_is_reported_and_corpus_continues("run", 60)

    def test_successful_corpus_still_agrees(self):
        status, output = self.run_with_timeout(None)
        self.assertEqual(status, 0, output)
        self.assertIn("differential: FULL AGREEMENT", output)


if __name__ == "__main__":
    unittest.main()
