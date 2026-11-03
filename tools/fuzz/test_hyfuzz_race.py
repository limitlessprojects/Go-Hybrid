import contextlib
import io
import json
import os
import random
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hyfuzz_race  # noqa: E402


SRC = """\
// corpus sample
// EXPECTED-VERDICT: ACCEPT
fn main() {
    m := map[string]int{}
    m["a"] = 1
    n := map[string]int{}
    alias := fn() { n["b"] = 2 }
    alias()
    if m["a"] > 0 { m["c"] = m["a"] }

    go fmt.println("ok")
}
"""


class MutatorTests(unittest.TestCase):
    def test_accept_population_filters_verdicts(self):
        with tempfile.TemporaryDirectory() as d:
            for name, verdict in (("a.hy", "ACCEPT"), ("b.hy", "REJECT"),
                                  ("c.hy", "ACCEPT-MANAGED"), ("d.txt", "ACCEPT")):
                with open(os.path.join(d, name), "w", encoding="utf-8") as f:
                    f.write("// EXPECTED-VERDICT: %s\n" % verdict)
            self.assertEqual(len(hyfuzz_race.load_accepts(d)), 2)

    def test_perm_is_a_rotation_of_the_same_statements(self):
        rng = random.Random(1)
        mutant, kind = hyfuzz_race.mutate(rng, SRC, kind="perm")
        self.assertEqual(kind, "perm")
        body = hyfuzz_race.split_main_body(mutant)
        base = hyfuzz_race.split_main_body(SRC)
        self.assertCountEqual(body, base, "perm must not add or drop stmts")
        self.assertNotEqual(body, base, "rotation must move something")

    def test_dup_never_duplicates_a_declaration(self):
        for seed in range(12):
            rng = random.Random(seed)
            mutant, kind = hyfuzz_race.mutate(rng, SRC, kind="dup")
            if kind != "dup":
                self.assertIsNotNone(kind)
                continue
            body = hyfuzz_race.split_main_body(mutant)
            base = hyfuzz_race.split_main_body(SRC)
            duped = [s for s in body if body.count(s) > base.count(s)]
            self.assertTrue(duped, "dup must duplicate something")
            for s in duped:
                self.assertNotIn(":=", s)

    def test_splice_only_uses_declared_bindings_and_adds_wiring(self):
        rng = random.Random(2)
        mutant, kind = hyfuzz_race.mutate(rng, SRC, kind="splice")
        self.assertEqual(kind, "splice")
        body = hyfuzz_race.split_main_body(mutant)
        joined = "\n".join(body)
        self.assertIn("hz_wkr", joined)
        # wiring is real: the kill channel is both made and released
        self.assertIn("k_hz := make(chan bool, 1)", joined)
        self.assertIn("k_hz <- true", joined)

    def test_mutations_are_deterministic(self):
        rng_a = random.Random(7)
        rng_b = random.Random(7)
        for _ in range(6):
            self.assertEqual(hyfuzz_race.mutate(rng_a, SRC),
                             hyfuzz_race.mutate(rng_b, SRC))

    def test_no_main_is_skipped(self):
        rng = random.Random(3)
        self.assertEqual(hyfuzz_race.mutate(rng, "fn f() {}\n"),
                         (None, "no-main"))


class OracleTests(unittest.TestCase):
    def test_accept_rule_is_exit_zero_without_hyb_e(self):
        with tempfile.TemporaryDirectory() as d:
            for blob, accepted in (
                ("OK (0 diagnostics)", True),
                ("error[HYB-E3001]: nope", False),
            ):
                with self.subTest(accepted=accepted), patch.object(
                    hyfuzz_race.subprocess, "run",
                    return_value=subprocess.CompletedProcess(
                        [], 0 if accepted else 1, blob, ""),
                ):
                    ok, bad = hyfuzz_race.checker_accepts(
                        "hyb", "case.hy", d, 10.0)
                    self.assertIs(bad, None)
                    self.assertIs(ok, accepted)

    def test_pipeline_reports_accept_when_no_race(self):
        seq = [
            subprocess.CompletedProcess([], 0, "OK (0 diagnostics)", ""),
            subprocess.CompletedProcess([], 0, "", ""),   # emit-go
            subprocess.CompletedProcess([], 0, "", ""),   # go build -race
            subprocess.CompletedProcess([], 0, "ok\n", ""),
        ]

        def fake_run(*a, **k):
            return seq.pop(0)

        with tempfile.TemporaryDirectory() as d:
            with patch.object(hyfuzz_race.subprocess, "run", fake_run), \
                 patch.object(hyfuzz_race.os, "listdir",
                              return_value=["case.go"]):
                self.assertEqual(
                    hyfuzz_race.race_oracle("hyb", "go", SRC, d, 10.0),
                    ("accept-clean", ""))

    def test_pipeline_falsifies_soundness_on_data_race(self):
        tail = "WARNING: DATA RACE\nRead at 0x..."
        seq = [
            subprocess.CompletedProcess([], 0, "OK (0 diagnostics)", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", tail),
        ]
        d_shift = [iter(seq)]

        def fake_run(*a, **k):
            return next(d_shift[0])

        with tempfile.TemporaryDirectory() as d:
            with patch.object(hyfuzz_race.subprocess, "run", fake_run), \
                 patch.object(hyfuzz_race.os, "listdir",
                              return_value=["case.go"]):
                self.assertEqual(
                    hyfuzz_race.race_oracle("hyb", "go", SRC, d, 10.0)[0],
                    "RACE")

    def test_bridge_import_is_reported_as_skip_not_failure(self):
        seq = [
            subprocess.CompletedProcess([], 0, "OK (0 diagnostics)", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess(
                [], 1, "", "a.go:5:2: package int8fmt is not in std"),
        ]
        shifts = [iter(seq)]

        def fake_run(*a, **k):
            return next(shifts[0])

        with tempfile.TemporaryDirectory() as d:
            with patch.object(hyfuzz_race.subprocess, "run", fake_run), \
                 patch.object(hyfuzz_race.os, "listdir",
                              return_value=["case.go"]):
                verdict, _ = hyfuzz_race.race_oracle(
                    "hyb", "go", SRC, d, 10.0)
        self.assertEqual(verdict, "skip-bridge")   # out of flat-build scope

    def test_reject_is_a_verdict_not_a_violation(self):
        seq = [subprocess.CompletedProcess([], 1, "error[HYB-E3001]: x", "")]
        shifts = [iter(seq)]

        def fake_run(*a, **k):
            return next(shifts[0])

        with tempfile.TemporaryDirectory() as d:
            with patch.object(hyfuzz_race.subprocess, "run", fake_run):
                self.assertEqual(
                    hyfuzz_race.race_oracle("hyb", "go", SRC, d, 10.0),
                    ("reject", ""))


class ArtifactTests(unittest.TestCase):
    def test_failure_artifact_is_data_only_and_hashed(self):
        with tempfile.TemporaryDirectory() as d:
            hyfuzz_race.save_failure(d, 5, 0, "splice", "RACE",
                                     "WARNING: DATA RACE", SRC)
            with open(os.path.join(d, "case-000000.hy"),
                      encoding="utf-8") as f:
                self.assertEqual(f.read(), SRC)
            with open(os.path.join(d, "case-000000.json"),
                      encoding="utf-8") as f:
                meta = json.load(f)
            self.assertEqual(meta["seed"], 5)
            self.assertEqual(meta["kind"], "splice")
            self.assertEqual(meta["verdict"], "RACE")


class MainSmokeTests(unittest.TestCase):
    def test_main_smoke_reports_and_exits_zero_without_races(self):
        seq = [subprocess.CompletedProcess([], 1, "error[HYB-E3001]: x", "")]

        def fake_run(*a, **k):
            return seq[-1]

        argv = ["hyfuzz_race.py", "--hyb", "hyb", "--corpus", ".",
                "--go", "go", "--cases", "2", "--seed", "3"]
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "s.hy"), "w", encoding="utf-8") as f:
                f.write(SRC)
            with open(os.path.join(d, "hyb"), "w", encoding="utf-8") as f:
                f.write("binary")   # resolution only; subprocess is mocked
            argv = ["hyfuzz_race.py", "--hyb", os.path.join(d, "hyb"),
                    "--corpus", d, "--go", "go", "--cases", "2", "--seed", "3"]
            fake_list = lambda p: (["s.hy"] if os.fspath(p) == d
                                   else ["case.go"])
            with patch.object(sys, "argv", argv), \
                 patch.object(hyfuzz_race.subprocess, "run", fake_run), \
                 patch.object(hyfuzz_race.os, "listdir", fake_list):
                out = io.StringIO()
                with contextlib.redirect_stdout(out), \
                     contextlib.redirect_stderr(io.StringIO()):
                    hyfuzz_race.main()
        self.assertIn("hyfuzz_race: 2 cases", out.getvalue())
        self.assertIn("reject=2", out.getvalue())

    def test_main_exits_one_on_race(self):
        tail = "WARNING: DATA RACE"
        seq = [
            subprocess.CompletedProcess([], 0, "OK (0 diagnostics)", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", ""),
            subprocess.CompletedProcess([], 0, "", tail),
        ]
        shifts = [iter(seq)]

        def fake_run(*a, **k):
            return next(shifts[0])

        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "s.hy"), "w", encoding="utf-8") as f:
                f.write(SRC)
            with open(os.path.join(d, "hyb"), "w", encoding="utf-8") as f:
                f.write("binary")
            argv = ["hyfuzz_race.py", "--hyb", os.path.join(d, "hyb"),
                    "--corpus", d, "--go", "go", "--cases", "1",
                    "--seed", "3", "--workers", "1"]
            fake_list = lambda p: (["s.hy"] if os.fspath(p) == d else ["case.go"])
            with patch.object(sys, "argv", argv), \
                 patch.object(hyfuzz_race.subprocess, "run", fake_run), \
                 patch.object(hyfuzz_race.os, "listdir", fake_list):
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf), \
                     contextlib.redirect_stderr(io.StringIO()):
                    with self.assertRaises(SystemExit) as ctx:
                        hyfuzz_race.main()
            self.assertEqual(ctx.exception.code, 1)
            self.assertIn("RACE", buf.getvalue())


if __name__ == "__main__":
    unittest.main()
