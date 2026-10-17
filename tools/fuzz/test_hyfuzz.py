import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import hyfuzz


class FuzzProcessTests(unittest.TestCase):
    def test_accept_and_clean_reject_are_valid(self):
        with tempfile.TemporaryDirectory() as workdir:
            for code in (0, 1):
                with self.subTest(code=code), patch.object(
                    hyfuzz.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], code, "", ""),
                ):
                    self.assertEqual(
                        hyfuzz.check_one("hyb", "fn main() {}", workdir, 1),
                        (None, code == 0),
                    )

    def test_signal_crashes_and_other_abnormal_exits_fail(self):
        with tempfile.TemporaryDirectory() as workdir:
            for code in (-11, -6, -9, 2, 101, 134, 139, 137, 3221225477):
                with self.subTest(code=code), patch.object(
                    hyfuzz.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], code, "", ""),
                ):
                    self.assertEqual(
                        hyfuzz.check_one("hyb", "fn main() {}", workdir, 1),
                        (f"UNEXPECTED exit={code}", False),
                    )

    def test_timeout_still_fails(self):
        with tempfile.TemporaryDirectory() as workdir, patch.object(
            hyfuzz.subprocess, "run",
            side_effect=subprocess.TimeoutExpired("hyb", 1),
        ):
            self.assertEqual(hyfuzz.check_one("hyb", "", workdir, 1), ("HANG", False))

    def test_internal_error_text_still_fails(self):
        with tempfile.TemporaryDirectory() as workdir:
            for text in hyfuzz.BAD_TEXT:
                with self.subTest(text=text), patch.object(
                    hyfuzz.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], 1, "", text),
                ):
                    bad, accepted = hyfuzz.check_one("hyb", "", workdir, 1)
                    self.assertTrue(bad.startswith("ICE:"))
                    self.assertFalse(accepted)

    def test_each_case_is_run_once_and_counted_from_that_run(self):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            for code in (0, 1):
                output = io.StringIO()
                with self.subTest(code=code), patch.object(sys, "argv", [
                    "hyfuzz.py", "--hyb", sys.executable, "--corpus", corpus, "--cases", "3",
                ]), patch.object(
                    hyfuzz.subprocess, "run",
                    return_value=subprocess.CompletedProcess([], code, "", ""),
                ) as run, contextlib.redirect_stdout(output):
                    hyfuzz.main()
                    self.assertEqual(run.call_count, 3)
                    self.assertIn(f"clean-accepts={3 if code == 0 else 0}", output.getvalue())
                    self.assertIn(f"clean-rejects={3 if code == 1 else 0}", output.getvalue())

    def test_real_unexpected_exit_cannot_report_success(self):
        # Python is deliberately not a checker: `python check case.hy`
        # exits 2 without a compiler panic message, on Windows and POSIX.
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            output = io.StringIO()
            with patch.object(sys, "argv", [
                "hyfuzz.py", "--hyb", sys.executable, "--corpus", corpus, "--cases", "1",
            ]), contextlib.redirect_stdout(output), self.assertRaises(SystemExit) as failure:
                hyfuzz.main()
            self.assertEqual(failure.exception.code, 1)
            self.assertIn("UNEXPECTED exit=2", output.getvalue())
            self.assertNotIn("all invariants held", output.getvalue())


class FuzzCaseCountTests(unittest.TestCase):
    def run_cases(self, requested, outcomes):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, 'case.hy').write_text('fn main() {}', encoding='utf-8')
            output = io.StringIO()
            with patch.object(sys, 'argv', [
                'hyfuzz.py', '--hyb', sys.executable, '--corpus', corpus,
                '--cases', str(requested), '--seed', '7',
            ]), patch.object(hyfuzz, 'gen_salad', return_value='salad'), \
                 patch.object(hyfuzz, 'gen_mutate', return_value='mutate'), \
                 patch.object(hyfuzz, 'gen_nest', return_value='nest'), \
                 patch.object(hyfuzz, 'check_one', side_effect=outcomes) as check, \
                 contextlib.redirect_stdout(output):
                status = 0
                try:
                    hyfuzz.main()
                except SystemExit as exc:
                    status = exc.code
            return status, output.getvalue(), check.call_count

    def test_failure_cap_reports_ten_executed_not_requested_1200(self):
        status, out, calls = self.run_cases(1200, [('HANG', False)] * 10)
        self.assertEqual(status, 1)
        self.assertEqual(calls, 10)
        self.assertIn('hyfuzz: 10 cases, seed=7,', out)
        self.assertIn('1190 requested cases not executed', out)
        self.assertIn('10 INVARIANT VIOLATIONS', out)
        self.assertNotIn('hyfuzz: 1200 cases,', out)
        self.assertNotIn('all invariants held', out)

    def test_early_stop_total_includes_clean_accepts_and_rejects(self):
        outcomes = [(None, True), (None, False), (None, False)]
        outcomes += [('UNEXPECTED exit=2', False)] * 10
        status, out, calls = self.run_cases(1200, outcomes)
        self.assertEqual(status, 1)
        self.assertEqual(calls, 13)
        self.assertIn('hyfuzz: 13 cases, seed=7,', out)
        self.assertIn('clean-rejects=2, clean-accepts=1', out)
        self.assertIn('1187 requested cases not executed', out)
        self.assertNotIn('all invariants held', out)

    def test_completed_success_keeps_existing_summary_format(self):
        status, out, calls = self.run_cases(3, [(None, True), (None, False), (None, True)])
        self.assertEqual(status, 0)
        self.assertEqual(calls, 3)
        self.assertIn('hyfuzz: 3 cases, seed=7, clean-rejects=1, clean-accepts=2', out)
        self.assertNotIn('not executed', out)
        self.assertIn('all invariants held', out)

    def test_completed_failure_batch_does_not_claim_early_stop(self):
        status, out, calls = self.run_cases(3, [('HANG', False), (None, False), (None, True)])
        self.assertEqual(status, 1)
        self.assertEqual(calls, 3)
        self.assertIn('hyfuzz: 3 cases, seed=7,', out)
        self.assertIn('1 INVARIANT VIOLATIONS', out)
        self.assertNotIn('not executed', out)
        self.assertNotIn('all invariants held', out)

    def test_tenth_failure_on_final_case_is_not_early_stop(self):
        status, out, calls = self.run_cases(10, [('HANG', False)] * 10)
        self.assertEqual(status, 1)
        self.assertEqual(calls, 10)
        self.assertIn('hyfuzz: 10 cases, seed=7,', out)
        self.assertNotIn('not executed', out)


    def test_real_cli_stops_at_ten_but_does_not_report_1200(self):
        # Python is deliberately not a checker: it exits 2 for the absent
        # `check` script. Exercise all ten real child invocations offline.
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, 'case.hy').write_text('fn main() {}', encoding='utf-8')
            result = subprocess.run([
                sys.executable, hyfuzz.__file__, '--hyb', sys.executable,
                '--corpus', corpus, '--cases', '1200', '--seed', '7',
            ], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count('UNEXPECTED exit=2'), 10)
        self.assertIn('hyfuzz: 10 cases, seed=7,', result.stdout)
        self.assertIn('1190 requested cases not executed', result.stdout)
        self.assertNotIn('all invariants held', result.stdout)


if __name__ == "__main__":
    unittest.main()
