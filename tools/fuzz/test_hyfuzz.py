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

    def test_oserror_during_check_fails(self):
        with tempfile.TemporaryDirectory() as workdir, patch.object(
            hyfuzz.subprocess, "run",
            side_effect=OSError("exec format error"),
        ):
            bad, accepted = hyfuzz.check_one("hyb", "", workdir, 1)
            self.assertTrue(bad.startswith("EXEC_ERROR:"))
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


class FuzzInputValidationTests(unittest.TestCase):
    def test_missing_hyb_binary_fails_closed(self):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            with patch.object(sys, "argv", [
                "hyfuzz.py", "--hyb", "/nonexistent/path/to/hyb", "--corpus", corpus, "--cases", "1",
            ]), self.assertRaises(SystemExit) as failure:
                hyfuzz.main()
            self.assertTrue(bool(failure.exception.code))

    def test_directory_hyb_fails_closed(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            with patch.object(sys, "argv", [
                "hyfuzz.py", "--hyb", d, "--corpus", corpus, "--cases", "1",
            ]), self.assertRaises(SystemExit) as failure:
                hyfuzz.main()
            self.assertTrue(bool(failure.exception.code))

    def test_missing_corpus_directory_fails_closed(self):
        with patch.object(sys, "argv", [
            "hyfuzz.py", "--hyb", sys.executable, "--corpus", "/nonexistent/corpus", "--cases", "1",
        ]), self.assertRaises(SystemExit) as failure:
            hyfuzz.main()
        self.assertTrue(bool(failure.exception.code))

    def test_file_corpus_fails_closed(self):
        with tempfile.NamedTemporaryFile() as tf:
            with patch.object(sys, "argv", [
                "hyfuzz.py", "--hyb", sys.executable, "--corpus", tf.name, "--cases", "1",
            ]), self.assertRaises(SystemExit) as failure:
                hyfuzz.main()
            self.assertTrue(bool(failure.exception.code))

    def test_non_positive_cases_fails_closed(self):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            for cases in ("0", "-5"):
                with self.subTest(cases=cases):
                    with patch.object(sys, "argv", [
                        "hyfuzz.py", "--hyb", sys.executable, "--corpus", corpus, "--cases", cases,
                    ]), self.assertRaises(SystemExit) as failure:
                        hyfuzz.main()
                    self.assertTrue(bool(failure.exception.code))

    def test_non_positive_timeout_fails_closed(self):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            for timeout in ("0", "-1.0", "nan", "inf", "-inf"):
                with self.subTest(timeout=timeout):
                    with patch.object(sys, "argv", [
                        "hyfuzz.py", "--hyb", sys.executable, "--corpus", corpus, "--cases", "1", "--timeout", timeout,
                    ]), self.assertRaises(SystemExit) as failure:
                        hyfuzz.main()
                    self.assertTrue(bool(failure.exception.code))

    def test_finite_positive_timeout_accepted(self):
        with tempfile.TemporaryDirectory() as corpus:
            Path(corpus, "case.hy").write_text("fn main() {}", encoding="utf-8")
            with patch.object(sys, "argv", [
                "hyfuzz.py", "--hyb", sys.executable, "--corpus", corpus, "--cases", "1", "--timeout", "2.5",
            ]), patch.object(hyfuzz, "check_one", return_value=(None, True)):
                # Returns cleanly without exiting with error
                hyfuzz.main()


if __name__ == "__main__":
    unittest.main()
