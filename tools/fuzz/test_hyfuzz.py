import contextlib
import hashlib
import io
import json
import random
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


class FuzzArtifactTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hyfuzz-artifacts-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.corpus = self.root / "corpus"
        self.corpus.mkdir()
        (self.corpus / "sample.hy").write_text("fn main() {}", encoding="utf-8")
        self.source = "// full reproduction \u65e5\u672c\u8a9e\n" + "x " * 200 + "TAIL_MARKER\n"
        self.checker = self.root / "checker"
        self.checker.write_text(
            "#!" + sys.executable + "\n"
            "from pathlib import Path\nimport sys\n"
            "source = Path(sys.argv[2]).read_text(encoding='utf-8')\n"
            "sys.exit(23 if source.endswith('TAIL_MARKER\\n') else 0)\n",
            encoding="utf-8",
        )
        self.checker.chmod(0o755)
        self.destination = self.root / "failures"

    def invoke(self, destination=True, source=None):
        args = [
            "hyfuzz.py", "--hyb", str(self.checker), "--corpus", str(self.corpus),
            "--cases", "1", "--seed", "470", "--timeout", "2",
        ]
        if destination:
            args += ["--failures-dir", str(self.destination)]
        stdout, stderr = io.StringIO(), io.StringIO()
        code = 0
        with patch.object(sys, "argv", args), patch.object(
            hyfuzz, "gen_salad", return_value=self.source if source is None else source,
        ), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                hyfuzz.main()
            except SystemExit as exc:
                code = exc.code
        return code, stdout.getvalue(), stderr.getvalue()

    def test_full_source_and_metadata_are_saved_and_replayable(self):
        code, stdout, stderr = self.invoke()
        self.assertEqual(code, 1, stdout + stderr)
        sources = list(self.destination.glob("hyfuzz-*/*.hy"))
        self.assertEqual(len(sources), 1)
        saved = sources[0]
        self.assertEqual(saved.read_text(encoding="utf-8"), self.source)
        metadata = json.loads(saved.with_suffix(".json").read_text(encoding="utf-8"))
        self.assertEqual(metadata["seed"], 470)
        self.assertEqual(metadata["case_index"], 0)
        self.assertEqual(metadata["kind"], 0)
        self.assertEqual(metadata["failure"], "UNEXPECTED exit=23")
        self.assertEqual(metadata["source_file"], saved.name)
        self.assertEqual(metadata["source_sha256"], hashlib.sha256(saved.read_bytes()).hexdigest())
        try:
            replay = subprocess.run(
                [str(self.checker), "check", str(saved)], capture_output=True, timeout=5,
            )
        except OSError as exc:
            if getattr(exc, "winerror", None) == 193:
                replay = subprocess.run(
                    [sys.executable, str(self.checker), "check", str(saved)],
                    capture_output=True, timeout=5,
                )
            else:
                raise
        self.assertEqual(replay.returncode, 23)
        self.assertIn(str(saved), stdout)

    def test_success_creates_no_output_directory(self):
        code, stdout, stderr = self.invoke(source="fn main() {}")
        self.assertEqual(code, 0, stdout + stderr)
        self.assertFalse(self.destination.exists())

    def test_repeated_seed_does_not_overwrite_existing_reproduction(self):
        self.assertEqual(self.invoke()[0], 1)
        before = {path: path.read_bytes() for path in self.destination.rglob("*") if path.is_file()}
        self.assertEqual(self.invoke()[0], 1)
        self.assertEqual(len(list(self.destination.glob("hyfuzz-*"))), 2)
        for path, data in before.items():
            self.assertEqual(path.read_bytes(), data)

    def test_output_io_error_is_visible_and_not_success(self):
        self.destination.write_text("existing user data", encoding="utf-8")
        code, stdout, stderr = self.invoke()
        self.assertEqual(code, 2, stdout + stderr)
        self.assertIn("cannot save failure artifact", stderr)
        self.assertNotIn("all invariants held", stdout)
        self.assertEqual(self.destination.read_text(encoding="utf-8"), "existing user data")

    def test_no_option_keeps_existing_failure_behavior(self):
        code, stdout, stderr = self.invoke(destination=False)
        self.assertEqual(code, 1, stdout + stderr)
        self.assertIn("UNEXPECTED exit=23", stdout)
        self.assertFalse(self.destination.exists())


class NestGenerationTests(unittest.TestCase):
    def test_branch_heavy_seed_keeps_one_deep_path(self):
        source = hyfuzz.gen_nest(random.Random(9498))
        self.assertEqual(source.count('fmt.println("deep")'), 1)
        self.assertLess(len(source), 4096)
        self.assertIn('else {', source)

    def test_all_branch_nesting_remains_linear(self):
        rng = random.Random(0)
        with patch.object(rng, 'randint', return_value=12), \
             patch.object(rng, 'choice', return_value=hyfuzz.NEST_SHAPES[3]):
            source = hyfuzz.gen_nest(rng)
        self.assertEqual(source.count('if x {'), 12)
        self.assertEqual(source.count('else {'), 12)
        self.assertEqual(source.count('fmt.println("deep")'), 1)
        self.assertLess(len(source), 4096)
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


class GoToolchainOracleTests(unittest.TestCase):
    def test_oracle_one_accepts_clean_build_and_flags_build_failures(self):
        with tempfile.TemporaryDirectory() as workdir:
            with patch.object(
                hyfuzz.subprocess, "run",
                return_value=subprocess.CompletedProcess([], 0, "built out\n", ""),
            ):
                self.assertEqual(
                    hyfuzz.oracle_one("hyb", "fn main() {}", workdir, 1),
                    (None, True),
                )
            with patch.object(
                hyfuzz.subprocess, "run",
                return_value=subprocess.CompletedProcess(
                    [], 1, "",
                    "go build failed:\n# command-line-arguments\n./out.go:4:6: declared and not used: x\n",
                ),
            ):
                bad, built = hyfuzz.oracle_one("hyb", "fn main() { var x int = 1 }", workdir, 1)
                self.assertFalse(built)
                self.assertEqual(
                    bad,
                    "ORACLE_BUILD_FAIL: ./out.go:4:6: declared and not used: x",
                )

    def test_oracle_one_timeout_and_ice_are_detected(self):
        with tempfile.TemporaryDirectory() as workdir:
            with patch.object(
                hyfuzz.subprocess, "run",
                side_effect=subprocess.TimeoutExpired("hyb", 1),
            ):
                self.assertEqual(
                    hyfuzz.oracle_one("hyb", "fn main() {}", workdir, 1),
                    ("ORACLE_HANG", False),
                )
            with patch.object(
                hyfuzz.subprocess, "run",
                return_value=subprocess.CompletedProcess([], 1, "", "thread 'main' panicked at emit"),
            ):
                bad, built = hyfuzz.oracle_one("hyb", "fn main() {}", workdir, 1)
                self.assertFalse(built)
                self.assertTrue(bad.startswith("ORACLE_ICE:"))

    def test_gen_go_decidable_is_deterministic_and_covers_all_families(self):
        rng_a = random.Random(20261018)
        rng_b = random.Random(20261018)
        seq_a = [hyfuzz.gen_go_decidable(rng_a) for _ in range(24)]
        seq_b = [hyfuzz.gen_go_decidable(rng_b) for _ in range(24)]
        self.assertEqual(seq_a, seq_b)
        for fam in hyfuzz.GO_DECIDABLE_FAMILIES:
            got_fam, src = hyfuzz.gen_go_decidable(random.Random(7), fam)
            self.assertEqual(got_fam, fam)
            self.assertIn("fn main()", src)

    def test_oracle_negative_controls_are_registered(self):
        names = [name for name, _ in hyfuzz.ORACLE_NEGATIVE_CONTROLS]
        self.assertEqual(
            names,
            [
                "slice_runtime_oob",
                "runtime_unary_neg_min_int8",
                "array_len_300_int8",
            ],
        )
        for _, src in hyfuzz.ORACLE_NEGATIVE_CONTROLS:
            self.assertIn("fn main()", src)


    def test_invariant_4_oracle_build_catches_accepted_unbuildable_case_and_writes_report(self):
        with tempfile.TemporaryDirectory() as root:
            corpus = Path(root, "corpus")
            corpus.mkdir()
            Path(corpus, "01-ok.hy").write_text(
                "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
            )
            Path(corpus, "02-rej.hy").write_text(
                "// EXPECTED-VERDICT: REJECT HYB-E6002\nfn main() { var x int8 = 300 }\n",
                encoding="utf-8",
            )
            report_path = Path(root, "oracle_report.json")

            def fake_run(cmd, **kwargs):
                subcmd = cmd[1]
                hy_file = Path(cmd[2])
                text = hy_file.read_text(encoding="utf-8")
                if subcmd == "check":
                    if "EXPECTED-VERDICT: REJECT" in text:
                        return subprocess.CompletedProcess(cmd, 1, "", "")
                    return subprocess.CompletedProcess(cmd, 0, "", "")
                if subcmd == "build":
                    # Simulate accepted-but-unbuildable on unused `var x`
                    if "var x" in text and "_ = x" not in text:
                        return subprocess.CompletedProcess(
                            cmd, 1, "",
                            "go build failed:\n./out.go:4:6: declared and not used: x\n",
                        )
                    return subprocess.CompletedProcess(cmd, 0, "built\n", "")
                return subprocess.CompletedProcess(cmd, 2, "", "unknown subcmd")

            output = io.StringIO()
            with patch.object(sys, "argv", [
                "hyfuzz.py",
                "--hyb", sys.executable,
                "--corpus", str(corpus),
                "--strategy", "go-decidable",
                "--oracle-build",
                "--audit-corpus",
                "--seed", "20261018",
                "--cases", "12",
                "--max-failures", "0",
                "--oracle-report", str(report_path),
            ]), patch.object(hyfuzz.subprocess, "run", side_effect=fake_run), \
                 contextlib.redirect_stdout(output), \
                 self.assertRaises(SystemExit) as failure:
                hyfuzz.main()

            self.assertEqual(failure.exception.code, 1)
            self.assertIn("ORACLE_BUILD_FAIL", output.getvalue())
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["corpus_audit"]["corpus_total"], 2)
            self.assertEqual(report["corpus_audit"]["accept_built"], 1)
            self.assertEqual(report["corpus_audit"]["reject_rejected"], 1)
            self.assertEqual(report["corpus_audit"]["accept_disagreement_rate"], 0.0)
            self.assertGreater(report["oracle_disagreements"], 0)
            self.assertGreater(
                report["per_family"]["buildable_decl"]["oracle_failed"], 0
            )


if __name__ == "__main__":
    unittest.main()
