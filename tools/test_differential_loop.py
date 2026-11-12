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
# patch.object(HARNESS_MOD.subprocess, "run", ...) patches the shared
# subprocess module, so a fake must call the real run through this handle.
REAL_RUN = subprocess.run


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
                capture_output=True, encoding="utf-8", errors="replace",
                timeout=10,
            )

    def assert_result(self, result, code):
        self.assertEqual(result.returncode, code, result.stdout + result.stderr)
        self.assertEqual("FULL AGREEMENT" in result.stdout, code == 0)

    def test_registered_fatal_is_allowed(self):
        self.assert_result(self.run_case(stderr="fatal error: concurrent map writes\n"), 0)

    def test_registered_fatal_before_long_stack_is_allowed(self):
        self.assert_result(self.run_case(
            stderr="fatal error: concurrent map writes\n" + "stack frame\n" * 100), 0)

    def test_swiss_map_attributed_corruption_fatal_is_allowed(self):
        # Go >= 1.26 swiss-map runtime self-attributes the torn write:
        # "small map with no empty slot (concurrent map writes?)".
        self.assert_result(self.run_case(
            stderr="fatal error: small map with no empty slot (concurrent map writes?)\n"), 0)

    def test_fault_address_fatal_is_concurrent_map_class(self):
        # The torn write can corrupt a slot pointer; the runtime then raises
        # an unattributed fault-address fatal from the same race.
        self.assert_result(self.run_case(
            stderr="fatal error: unexpected fault address 0x0\n"
                   "[signal SIGSEGV: segmentation violation code=0x1 addr=0x0 pc=0x1]\n"), 0)

    def test_fault_address_fatal_is_not_other_hazards(self):
        self.assert_result(self.run_case(
            stderr="fatal error: unexpected fault address 0x0\n",
            hazard="all goroutines are asleep - deadlock!"), 1)

    def test_panic_mentioning_hazard_phrase_is_not_evidence(self):
        self.assert_result(self.run_case(
            stderr="panic: unexpected nil pointer mentioning concurrent map writes\n"), 1)

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
                    capture_output=True, encoding="utf-8", errors="replace",
                    timeout=10,
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


class CheckerUtf8Tests(unittest.TestCase):
    """checker_rejects must read checker JSON as UTF-8, not the host locale.

    The checker's JSON is UTF-8 by contract; decoding it with a legacy
    locale (Windows cp936) corrupts or rejects valid non-ASCII
    diagnostics. These drive the real function through real OS pipes with
    the legacy fallback forced, so Linux CI exercises the Windows failure
    without changing the global locale.
    """

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = pathlib.Path(self.directory.name)
        self.checker = self.root / "checker.py"
        # Emit exact bytes, independently of the child's own locale. The
        # non-ASCII case carries its discriminator in tier0_rule: under a
        # mojibake decode the rule comparison flips, so the test pins the
        # decoded content, not merely the absence of a crash.
        self.checker.write_text(
            f"#!{sys.executable}\n"
            "import json, sys\n"
            "case = sys.argv[2]\n"
            "if case.endswith('invalid.hy'):\n"
            "    sys.stdout.buffer.write(b'\\xff')\n"
            "elif case.endswith('weird-exit.hy'):\n"
            "    raise SystemExit(3)\n"
            "else:\n"
            "    rule = 'A1' if case.endswith('ascii.hy') else "
            "('B2' if case.endswith('mismatch.hy') else '\\u951f')\n"
            "    value = {'summary': {'verdict': 'reject'},\n"
            "             'diagnostics': [{'severity': 'error',\n"
            "                             'tier0_rule': rule, 'message': 'note'}]}\n"
            "    sys.stdout.buffer.write("
            "json.dumps(value, ensure_ascii=False).encode('utf-8'))\n"
            "    raise SystemExit(1)\n",
            encoding="utf-8")

    def run_with_legacy_locale(self, command, **kwargs):
        # Keep real OS pipes/decoding. Select the legacy fallback on every
        # host so Linux CI also exercises the Windows cp936 failure,
        # without changing global locale or mocking the checker output.
        # Note the blast radius: patching HARNESS_MOD.subprocess.run
        # patches the shared subprocess module, so this wrapper sees every
        # subprocess.run issued while a case is in scope.
        if kwargs.get("text") or kwargs.get("universal_newlines"):
            kwargs.setdefault("encoding", "cp936")
        return REAL_RUN([sys.executable, command[0], *command[1:]], **kwargs)

    def call_checker(self, name, expected_rule="A1"):
        with patch.object(HARNESS_MOD.subprocess, "run",
                          self.run_with_legacy_locale):
            return HARNESS_MOD.checker_rejects(
                str(self.checker), name, expected_rule)

    def test_utf8_rule_survives_non_utf8_locale(self):
        # The discriminator is non-ASCII; a mojibake decode would compare
        # a different rule and flip the verdict to False.
        ok, detail = self.call_checker("valid.hy", expected_rule="\u951f")
        self.assertTrue(ok, detail)

    def test_ascii_json_keeps_the_existing_contract(self):
        ok, detail = self.call_checker("ascii.hy")
        self.assertTrue(ok, detail)

    def test_invalid_utf8_output_is_reported_not_raised(self):
        ok, detail = self.call_checker("invalid.hy")
        self.assertFalse(ok)
        self.assertIn("not valid UTF-8", detail)

    def test_unexpected_exit_still_reported(self):
        ok, detail = self.call_checker("weird-exit.hy")
        self.assertFalse(ok)
        self.assertIn("unexpected checker exit 3", detail)

    def test_checker_that_cannot_run_is_reported(self):
        # Not patched: a missing checker raises OSError out of run(), which
        # must surface as a reported (False, detail), never a traceback
        # that aborts the whole differential loop.
        ok, detail = HARNESS_MOD.checker_rejects(
            str(self.root / "missing-checker"), "valid.hy", "A1")
        self.assertFalse(ok)
        self.assertIn("checker could not be run", detail)

    def test_rule_mismatch_still_reported(self):
        ok, detail = self.call_checker("mismatch.hy", expected_rule="A1")
        self.assertFalse(ok)
        self.assertIn("first_rule=B2", detail)

    @unittest.skipUnless(os.name == "posix", "CLI fixture uses POSIX executable scripts")
    def test_invalid_utf8_output_does_not_crash_the_loop(self):
        corpus = self.root / "corpus"
        corpus.mkdir()
        (corpus / "01-invalid.hy").write_text(
            "// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n", encoding="utf-8")
        (corpus / "02-valid.hy").write_text(
            "// EXPECTED-VERDICT: REJECT A1\nfn main() {}\n", encoding="utf-8")
        self.checker.chmod(0o755)
        result = subprocess.run(
            [sys.executable, str(HARNESS), "--bin", str(self.checker),
             "--corpus", str(corpus)],
            capture_output=True, encoding="utf-8", errors="replace",
            timeout=10)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("not valid UTF-8", result.stdout)
        self.assertIn("02-valid.hy", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()


class TimeoutCorpusTests(unittest.TestCase):
    """pr-468: a timed-out checker/build/program is one failed case; the
    corpus continues and the aggregate report still prints (#463 class)."""

    def run_with_timeout(self, timeout_stage):
        """The first case times out at `timeout_stage`; the second must still run."""
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
        self.assertNotIn("Traceback", output)

    def test_checker_timeout_reports_failure_and_continues(self):
        # main's checker leg reports the fault through the #492 handler;
        # the message carries the same "timed out after" wording.
        status, output = self.run_with_timeout("checker")
        self.assertEqual(status, 1, output)
        self.assertIn("FAIL  01-timeout.hy", output)
        self.assertIn("checker could not be run", output)
        self.assertIn("timed out after 60 seconds", output)
        self.assertIn("PASS  02-ok.hy", output)
        self.assertIn("differential: 1 disagreement(s)", output)
        self.assertNotIn("FULL AGREEMENT", output)

    def test_build_timeout_reports_failure_and_continues(self):
        self.assert_timeout_is_reported_and_corpus_continues("build", 300)

    def test_program_timeout_reports_failure_and_continues(self):
        self.assert_timeout_is_reported_and_corpus_continues("run", 60)

    def test_successful_corpus_still_agrees(self):
        status, output = self.run_with_timeout(None)
        self.assertEqual(status, 0, output)
        self.assertIn("differential: FULL AGREEMENT", output)


# A race report shaped like the real thing: go1.26's swiss-map runtime names
# the contended object through mapaccess1_faststr / mapassign_faststr, and
# never uses the word "writes" that the corpus hazard registers. That
# vocabulary mismatch is the whole reason documented_race() exists.
RACE_ON_MAP = """==================
WARNING: DATA RACE
Read at 0x00c00009c0f0 by goroutine 8:
  runtime.mapaccess1_faststr()
      /home/user/tools/go/src/internal/runtime/maps/runtime_faststr.go:101 +0x0
  main.tally()
      /tmp/case/rb.go:21 +0xa4
  main.main.gowrap1()
      /tmp/case/rb.go:33 +0x12

Previous write at 0x00c00009c0f0 by goroutine 11:
  runtime.mapassign_faststr()
      /home/user/tools/go/src/internal/runtime/maps/runtime_faststr.go:263 +0x0
  main.tally()
      /tmp/case/rb.go:21 +0xd6
  main.main.gowrap1()
      /tmp/case/rb.go:33 +0x12

Goroutine 8 (running) created at:
  main.main()
      /tmp/case/rb.go:33 +0x8f
==================
"""

RACE_ON_SLICE = """==================
WARNING: DATA RACE
Write at 0x00c000112080 by goroutine 7:
  main.bump()
      /tmp/case/rb.go:14 +0x51

Previous read at 0x00c000112080 by goroutine 9:
  main.reader()
      /tmp/case/rb.go:22 +0x3c
==================
"""


class RaceDetectorTests(unittest.TestCase):
    """`--race` links the race detector and fails on undocumented races.

    The plain loop only ever sees a hazard when the runtime aborts. The
    detector sees the same defect while the program still exits 0, which is
    the gap this option closes.
    """

    def run_race_case(self, stderr, hazards=(), stdout="", exit_code=0):
        build = subprocess.CompletedProcess([], 0, "", "")
        run = subprocess.CompletedProcess([], exit_code, stdout, stderr)
        with patch.object(HARNESS_MOD.subprocess, "run",
                          side_effect=[build, run]), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            return HARNESS_MOD.build_and_run(
                "hyb", "case.hy", ".", list(hazards), race=True)

    def test_race_with_no_registered_hazard_fails(self):
        ok, detail = self.run_race_case(RACE_ON_MAP)
        self.assertFalse(ok)
        self.assertIn("data race detected", detail)

    def test_race_matching_registered_hazard_is_documented(self):
        ok, detail = self.run_race_case(
            RACE_ON_MAP, ["concurrent map writes (Go fatal, pre-registered)"])
        self.assertTrue(ok, detail)
        self.assertIn("documented RUNTIME-HAZARD race", detail)

    def test_race_on_other_object_is_not_covered_by_map_hazard(self):
        ok, detail = self.run_race_case(
            RACE_ON_SLICE, ["concurrent map writes (Go fatal, pre-registered)"])
        self.assertFalse(ok)
        self.assertIn("data race detected", detail)

    def test_detector_silent_means_no_race_failure(self):
        ok, detail = self.run_race_case("", ["concurrent map writes"])
        self.assertTrue(ok, detail)
        self.assertEqual(detail, "exit 0")

    def test_race_ignored_when_option_off(self):
        """Without race=True a race report must not change the verdict.

        The default loop is byte-for-byte the behaviour it had before; the
        new capability is strictly opt-in.
        """
        build = subprocess.CompletedProcess([], 0, "", "")
        run = subprocess.CompletedProcess([], 0, "", RACE_ON_MAP)
        with patch.object(HARNESS_MOD.subprocess, "run",
                          side_effect=[build, run]), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            ok, detail = HARNESS_MOD.build_and_run("hyb", "case.hy", ".")
        self.assertTrue(ok)
        self.assertEqual(detail, "exit 0")


class RaceFlagPlumbingTests(unittest.TestCase):
    """`--race` reaches `hyb build`, and only when asked for."""

    def capture_build_cmd(self, race=False):
        """Capture the argv handed to `hyb build` on the first subprocess call.

        The harness makes two calls — first `hyb build`, then the built
        binary. A call counter is used rather than a side_effect *list*,
        because a CompletedProcess sitting in that list is not a callable
        and mock would hand the function object back as the result.
        """
        captured = {}
        state = {"calls": 0}

        def fake_run(cmd, *a, **kw):
            state["calls"] += 1
            if state["calls"] == 1:
                captured["cmd"] = list(cmd)
            return subprocess.CompletedProcess(list(cmd), 0, "", "")

        with patch.object(HARNESS_MOD.subprocess, "run", side_effect=fake_run), \
             patch.object(HARNESS_MOD.os.path, "exists", return_value=True):
            HARNESS_MOD.build_and_run("hyb", "case.hy", ".", race=race)
        return captured.get("cmd", [])

    def test_race_flag_present_when_enabled(self):
        self.assertIn("--race", self.capture_build_cmd(race=True))

    def test_race_flag_absent_by_default(self):
        self.assertNotIn("--race", self.capture_build_cmd(race=False))

    def test_race_flag_does_not_displace_the_input_file(self):
        cmd = self.capture_build_cmd(race=True)
        self.assertEqual(cmd[0], "hyb")
        self.assertEqual(cmd[1], "build")
        self.assertIn("case.hy", cmd)


@unittest.skipUnless(os.name == "posix", "fixture programs use executable scripts")
class AcceptOutputDecodeTests(unittest.TestCase):
    """Real child-process decode faults must leave the corpus report intact."""

    def check_corpus(self, stage, stream, payload, expected_status):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            corpus = root / "cases"
            corpus.mkdir()
            expected = "\u03bb"
            for name in ("01-fault.hy", "02-control.hy"):
                (corpus / name).write_text(
                    "// EXPECTED-VERDICT: ACCEPT\n"
                    f"// EXPECTED-OUTPUT: {expected}\nfn main() {{}}\n",
                    encoding="utf-8")
            # Choose the faulty leg independently from the harness, and write
            # bytes through real pipes so subprocess performs the decoding.
            program = (
                f"#!{sys.executable}\nimport os\n"
                f"os.write(1, {expected.encode('utf-8')!r})\n"
            )
            bad_program = program + f"os.write({stream}, {payload!r})\n"
            builder = root / "builder"
            builder.write_text("\n".join([
                f"#!{sys.executable}",
                "import os, pathlib, sys",
                "fault = pathlib.Path(sys.argv[2]).name.startswith('01-')",
                f"if fault and {stage!r} == 'build':",
                f"    os.write({stream}, {payload!r})",
                f"body = {bad_program!r} if fault and {stage!r} == 'run' else {program!r}",
                "binary = pathlib.Path('a.out')",
                "binary.write_text(body, encoding='utf-8')",
                "binary.chmod(0o700)",
                "",
            ]), encoding="utf-8")
            builder.chmod(0o700)
            result = subprocess.run(
                [sys.executable, str(HARNESS), "--corpus", str(corpus),
                 "--bin", str(builder)],
                env={**os.environ, "PYTHONUTF8": "1"},
                capture_output=True, encoding="utf-8", timeout=15)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, expected_status, output)
        self.assertIn("PASS  02-control.hy", output)
        self.assertNotIn("Traceback", output)
        if expected_status:
            self.assertIn("FAIL  01-fault.hy", output)
            self.assertIn(f"{stage} output could not be decoded", output)
            self.assertIn("differential accept-side (build+run): 1/2", output)
            self.assertIn("differential: 1 disagreement(s)", output)
            self.assertNotIn("FULL AGREEMENT", output)
        else:
            self.assertIn("differential accept-side (build+run): 2/2", output)
            self.assertIn("differential: FULL AGREEMENT", output)

    def test_invalid_bytes_fail_one_case_on_each_accept_leg_and_stream(self):
        for stage in ("build", "run"):
            for stream in (1, 2):
                with self.subTest(stage=stage, stream=stream):
                    self.check_corpus(stage, stream, b"\xff", 1)

    def test_valid_multibyte_output_preserves_exact_output_matching(self):
        # Unicode diagnostics are harmless, while stdout must still match.
        for stage in ("build", "run"):
            with self.subTest(stage=stage):
                self.check_corpus(stage, 2, "\u03bb".encode("utf-8"), 0)
