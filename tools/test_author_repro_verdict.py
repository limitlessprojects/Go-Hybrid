"""Offline integration tests for author-stage's repro rejection gate.

Git operations use a real throwaway repository. The compiler and model are
explicit stubs: these tests verify gate orchestration, not compiler behavior.
"""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "author_controls_subject", Path(__file__).with_name("author_stage.py")
)
AUTHOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUTHOR)

SPECIALIZED_SPEC = importlib.util.spec_from_file_location(
    "specialized_verdict_subject", Path(__file__).with_name("specialized_brain.py")
)
SPECIALIZED = importlib.util.module_from_spec(SPECIALIZED_SPEC)
SPECIALIZED_SPEC.loader.exec_module(SPECIALIZED)

AUTOLAND_SPEC = importlib.util.spec_from_file_location(
    "autoland_verdict_subject", Path(__file__).with_name("autoland.py")
)
AUTOLAND = importlib.util.module_from_spec(AUTOLAND_SPEC)
AUTOLAND_SPEC.loader.exec_module(AUTOLAND)

CONTROL = "tests/checker-corpus/accept.hy"
DIFF = """diff --git a/compiler/example.rs b/compiler/example.rs
--- a/compiler/example.rs
+++ b/compiler/example.rs
@@ -1 +1 @@
-// before
+// after
"""


class AuthorReproVerdictTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="author-controls-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.queue = self.root / "automation/land-queue"
        self.out = self.queue / "author-123"
        self.blocked = self.root / "target/author-blocked"
        self.repro_dir = self.root / "compiler/target/author-stage"
        self.git("init", "-q")
        self.git("config", "user.name", "Test Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write(".gitignore", "target/\ncompiler/target/\n")
        self.write("compiler/example.rs", "// before\n")

    def git(self, *args):
        return subprocess.run(
            ["git", *args], cwd=self.root, check=True, capture_output=True,
            text=True, timeout=10,
        )

    def write(self, path, content):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def run_author(self, status=1, output="error[HYB-E6002]: rejected", *, expected=True):
        self.write(CONTROL, "fn main() {}\n")
        self.commit()
        original_sh = AUTHOR.sh
        checker_bin = self.root / "target/fake-hyb"
        checker_bin.parent.mkdir(parents=True, exist_ok=True)
        checker_bin.write_text(
            "#!" + sys.executable + "\nimport sys, os, signal\n"
            + "if 'control-' in sys.argv[2]: sys.exit(0)\n"
            + "print(" + repr(output) + ", flush=True)\n"
            + ("os.kill(os.getpid(), signal.SIGTERM)\n" if status < 0
               else "sys.exit(" + str(status) + ")\n"), encoding="utf-8",
        )
        checker_bin.chmod(0o755)

        def shell(args, **kwargs):
            if args[:2] == ["cargo", "build"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            return original_sh(args, **kwargs)

        with contextlib.ExitStack() as stack:
            for name, value in {
                "ROOT": self.root, "COMPILER": self.root / "compiler",
                "QUEUE": self.queue, "BLOCKED_DIR": self.blocked,
                "REPRO_DIR": self.repro_dir, "ACCEPT_CONTROLS": [CONTROL],
                "HYB": checker_bin,
            }.items():
                stack.enter_context(patch.object(AUTHOR, name, value))
            stack.enter_context(patch.dict(os.environ, {
                "AUTHOR_API_BASE": "https://fixture.invalid",
                "AUTHOR_MODEL": "offline-test-stub",
            }))
            stack.enter_context(patch.object(AUTHOR, "sh", side_effect=shell))
            stack.enter_context(patch.object(AUTHOR, "api_get", return_value={
                "number": 123, "title": "fixture", "body": "```hy\nfn main() {}\n```" + (" HYB-E6002" if expected else ""),
            }))
            brain = stack.enter_context(patch.object(
                AUTHOR, "chat", return_value="```diff\n" + DIFF + "```"
            ))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            result = AUTHOR.run_author(123, self.out, 1)
        return result

    def assertBlocked(self, result):
        self.assertEqual(result, 3)
        self.assertFalse(self.out.exists(), "a failed checker must never yield READY")
        self.assertEqual((self.root / "compiler/example.rs").read_text(), "// before\n")
        meta = json.loads((self.blocked / "issue-123/meta.json").read_text())
        self.assertEqual(meta["status"], "blocked")

    def test_exit_two_with_expected_code_is_not_a_rejection(self):
        self.assertBlocked(self.run_author(status=2))

    def test_signal_after_expected_diagnostic_is_not_a_rejection(self):
        self.assertBlocked(self.run_author(status=-15))

    def test_longer_diagnostic_code_does_not_match_expected(self):
        self.assertBlocked(self.run_author(output="error[HYB-E60020]: different code"))

    def test_missing_expected_code_still_blocks(self):
        self.assertBlocked(self.run_author(output="error[HYB-E1002]: different code"))

    def test_success_exit_with_expected_code_is_still_ineffective(self):
        self.assertBlocked(self.run_author(status=0))

    def test_exit_one_with_exact_expected_code_can_be_ready(self):
        self.assertEqual(self.run_author(), 0)
        meta = json.loads((self.out / "meta.json").read_text())
        self.assertEqual(meta["gates"]["patched"], "REJECT HYB-E6002")
        self.assertIn("// after", (self.out / "fix.patch").read_text())

    def test_unexpected_exit_also_blocks_without_expected_code(self):
        self.assertBlocked(self.run_author(status=2, expected=False))

    def test_exit_one_without_expected_code_keeps_existing_contract(self):
        self.assertEqual(self.run_author(output="rejected", expected=False), 0)


class SpecializedReproVerdictTests(unittest.TestCase):
    """Real checker processes; Cargo and later corpus gates are explicit stubs."""

    def run_smoke(self, status=1, output="error[HYB-E6002]: rejected", expected="HYB-E6002"):
        with tempfile.TemporaryDirectory(prefix="specialized-verdict-") as directory:
            root = Path(directory)
            checker = root / "fake-hyb"
            checker.write_text(
                "#!" + sys.executable + "\nimport sys, os, signal\n"
                + "print(" + repr(output) + ", flush=True)\n"
                + ("os.kill(os.getpid(), signal.SIGTERM)\n" if status < 0
                   else "sys.exit(" + str(status) + ")\n"), encoding="utf-8",
            )
            checker.chmod(0o755)
            real_sh = SPECIALIZED.sh
            later_gates = []

            def shell(args, **kwargs):
                if args[0] == "cargo":
                    return subprocess.CompletedProcess(args, 0, "", "")
                if args[0] == "python3":
                    later_gates.append(args[1])
                    return subprocess.CompletedProcess(args, 0, "", "")
                return real_sh(args, **kwargs)

            with contextlib.ExitStack() as stack:
                for name, value in {
                    "ROOT": root, "COMPILER": root, "HYB": checker,
                    "SMOKE_DIR": root / "smoke", "ACCEPT_CONTROLS": [],
                }.items():
                    stack.enter_context(patch.object(SPECIALIZED, name, value))
                stack.enter_context(patch.object(SPECIALIZED, "sh", side_effect=shell))
                verdict, reason = SPECIALIZED.smoke("repro.hy", expected, [])
            return verdict, reason, later_gates

    def assert_not_rejection(self, **kwargs):
        verdict, reason, later = self.run_smoke(**kwargs)
        self.assertFalse(verdict, reason)
        self.assertEqual(later, [], "invalid repro evidence must stop before later gates")

    def test_invocation_error_with_code_stops(self):
        self.assert_not_rejection(status=2)

    def test_signal_with_code_stops(self):
        self.assert_not_rejection(status=-15)

    def test_longer_code_stops(self):
        self.assert_not_rejection(output="error[HYB-E60020]: rejected")

    def test_invocation_error_without_expected_code_stops(self):
        self.assert_not_rejection(status=2, expected="")

    def test_accept_exit_still_stops(self):
        self.assert_not_rejection(status=0)

    def test_missing_code_still_stops(self):
        self.assert_not_rejection(output="error[HYB-E1002]: rejected")

    def test_exact_rejection_reaches_later_gates(self):
        verdict, reason, later = self.run_smoke()
        self.assertTrue(verdict, reason)
        self.assertEqual(later, ["tools/corpus_harness.py", "tools/check_corpus_census.py"])

    def test_unspecified_rejection_keeps_contract(self):
        verdict, reason, later = self.run_smoke(output="rejected", expected="")
        self.assertTrue(verdict, reason)
        self.assertEqual(len(later), 2)

    def test_surface_edits_updates_both_numbers_in_dual_count_surfaces(self):
        fixture_path = Path(__file__).parent / "tests" / "specialized_brain_census_cases.json"
        spec = json.loads(fixture_path.read_text(encoding="utf-8"))
        surfaces = spec["surfaces"]
        expected = spec["expected_replacements"]
        with patch.object(SPECIALIZED, "head", side_effect=lambda p: surfaces[p]):
            edits = SPECIALIZED.surface_edits(999, "99-fixture.hy", "Regression fixture.")
        by_path = {path: new for path, _old, new, *_rest in edits}
        for path, exp_new in expected.items():
            self.assertEqual(by_path[path], exp_new, f"dual-count drift in {path}")


class AutolandReproVerdictTests(unittest.TestCase):
    """Offline tests for autoland's baseline/patched repro gates and log_event CLI args."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="autoland-verdict-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.queue = self.root / "automation/land-queue"
        self.bundle = self.queue / "author-123"
        self.bundle.mkdir(parents=True, exist_ok=True)
        self.git("init", "-q")
        self.git("config", "user.name", "Test Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write("compiler/example.rs", "// before\n")
        self.write("automation/land-queue/author-123/repro.hy", "fn main() {}\n")
        self.write("automation/land-queue/author-123/fix.patch", DIFF)
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")

    def git(self, *args):
        return subprocess.run(
            ["git", *args], cwd=self.root, check=True, capture_output=True,
            text=True, timeout=10,
        )

    def write(self, path, content):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content, encoding="utf-8")

    def run_autoland(
        self,
        *,
        baseline=(0, ""),
        patched=(1, "error[HYB-E6002]: rejected"),
        expected="HYB-E6002",
        live=True,
    ):
        self.write(
            "automation/land-queue/author-123/meta.json",
            json.dumps({
                "issue": 123,
                "expected_code": expected,
                "basis": "fixture",
                "status": "ready",
            }) + "\n",
        )
        original_sh = AUTOLAND.sh
        checker_calls = []
        logged_cmds = []
        battery_called = []
        land_called = []

        def fake_check(repro_rel):
            idx = len(checker_calls)
            checker_calls.append(repro_rel)
            return baseline if idx == 0 else patched

        def fake_sh(args, **kwargs):
            if args[:2] == ["cargo", "build"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            if args[:2] == ["python3", "tools/shepherd.py"]:
                logged_cmds.append(list(args))
                return subprocess.CompletedProcess(args, 0, "", "")
            if args[:2] == ["git", "push"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            return original_sh(args, **kwargs)

        def fake_battery():
            battery_called.append(True)
            return [("workspace", True, "")]

        def fake_land(slug, meta, repro_rel):
            land_called.append(slug)
            return "deadbeef1234567890"

        with contextlib.ExitStack() as stack:
            for name, value in {
                "ROOT": self.root,
                "QUEUE": self.queue,
                "COMPILER": self.root / "compiler",
            }.items():
                stack.enter_context(patch.object(AUTOLAND, name, value))
            stack.enter_context(patch.object(AUTOLAND, "sh", side_effect=fake_sh))
            stack.enter_context(patch.object(AUTOLAND, "check_repro", side_effect=fake_check))
            stack.enter_context(patch.object(AUTOLAND, "battery", side_effect=fake_battery))
            stack.enter_context(patch.object(AUTOLAND, "land_commit", side_effect=fake_land))
            out_buf = io.StringIO()
            stack.enter_context(contextlib.redirect_stdout(out_buf))
            rc = AUTOLAND.run_bundle("author-123", live=live, token_env="GITHUB_TOKEN")
            AUTOLAND.revert_on_failure()
        return rc, out_buf.getvalue(), logged_cmds, battery_called, land_called

    def test_baseline_exit_one_is_stale_premise(self):
        rc, out, logs, battery, land = self.run_autoland(baseline=(1, "error[HYB-E6002]: already fixed"))
        self.assertEqual(rc, 3)
        self.assertIn("AUTOLAND-STALE-PREMISE", out)
        self.assertEqual(battery, [])
        self.assertEqual(land, [])
        self.assertEqual(len(logs), 1)
        self.assertIn("--pr", logs[0])
        self.assertNotIn("--issue", logs[0])

    def test_baseline_invocation_error_is_checker_failure_not_stale_premise(self):
        rc, out, logs, battery, land = self.run_autoland(baseline=(2, "hyb: cannot open file"))
        self.assertEqual(rc, 8)
        self.assertIn("AUTOLAND-CHECKER-FAILED", out)
        self.assertNotIn("STALE-PREMISE", out)
        self.assertEqual(battery, [])
        self.assertEqual(land, [])
        self.assertIn("blocked", logs[0])

    def test_patched_exit_two_with_expected_code_blocks_before_battery(self):
        rc, out, logs, battery, land = self.run_autoland(patched=(2, "error[HYB-E6002]: crash"))
        self.assertEqual(rc, 8)
        self.assertIn("AUTOLAND-CHECKER-FAILED", out)
        self.assertEqual(battery, [])
        self.assertEqual(land, [])

    def test_patched_signal_with_expected_code_blocks_before_battery(self):
        rc, out, logs, battery, land = self.run_autoland(patched=(-15, "error[HYB-E6002]: sigterm"))
        self.assertEqual(rc, 8)
        self.assertIn("AUTOLAND-CHECKER-FAILED", out)
        self.assertEqual(battery, [])
        self.assertEqual(land, [])

    def test_patched_longer_diagnostic_code_does_not_match_expected(self):
        rc, out, logs, battery, land = self.run_autoland(patched=(1, "error[HYB-E60020]: wrong code"))
        self.assertEqual(rc, 6)
        self.assertIn("AUTOLAND-WRONG-CODE", out)
        self.assertEqual(battery, [])
        self.assertEqual(land, [])

    def test_patched_exact_expected_code_reaches_battery_and_lands(self):
        rc, out, logs, battery, land = self.run_autoland(patched=(1, "error[HYB-E6002]: rejected"))
        self.assertEqual(rc, 0)
        self.assertIn("AUTOLAND-LANDED", out)
        self.assertEqual(len(battery), 1)
        self.assertEqual(land, ["author-123"])
        self.assertEqual(len(logs), 1)
        self.assertEqual(logs[0][-2:], ["--pr", "123"])


if __name__ == "__main__":
    unittest.main()
