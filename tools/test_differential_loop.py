"""Offline CLI regressions for the differential harness, using a fake builder.

These exercise harness decisions, not Hybrid compiler correctness.
Run: python3 -m unittest discover -s tools -p test_differential_loop.py -v
"""
import importlib.util
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


if __name__ == "__main__":
    unittest.main()
