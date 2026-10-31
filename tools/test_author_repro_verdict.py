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
    "autoland_subject", Path(__file__).with_name("autoland.py")
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
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="autoland-verdict-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.queue = self.root / "automation/land-queue"
        self.slug = "bundle-123"
        self.bundle_dir = self.queue / self.slug
        self.bundle_dir.mkdir(parents=True, exist_ok=True)
        self.git("init", "-q")
        self.git("config", "user.name", "Test Fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write("compiler/example.rs", "// before\n")
        self.commit()

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

    def run_autoland(self, baseline_res=(0, ""), patched_res=(1, "error[HYB-E6002]: rejected"), expected="HYB-E6002"):
        self.git("checkout", "--", ".")
        self.git("clean", "-fd")
        self.bundle_dir.mkdir(parents=True, exist_ok=True)
        meta = {
            "issue": 123,
            "expected_code": expected,
            "basis": "autoland-test",
            "status": "pending",
        }
        (self.bundle_dir / "meta.json").write_text(json.dumps(meta, indent=2))
        (self.bundle_dir / "fix.patch").write_text(DIFF)
        (self.bundle_dir / "repro.hy").write_text("fn main() {}\n")

        original_sh = AUTOLAND.sh
        call_count = [0]

        def fake_check_repro(repro_rel):
            call_count[0] += 1
            if call_count[0] == 1:
                return baseline_res
            return patched_res

        def shell(args, **kwargs):
            if args[:2] == ["git", "push"]:
                return subprocess.CompletedProcess(args, 0, "", "")
            return original_sh(args, **kwargs)

        with contextlib.ExitStack() as stack:
            for name, value in {
                "ROOT": self.root, "COMPILER": self.root / "compiler",
                "QUEUE": self.queue, "build": lambda: None,
                "check_repro": fake_check_repro,
                "battery": lambda: [("mock_gate", True, "")],
                "land_commit": lambda slug, meta, repro: "1234567890abcdef",
            }.items():
                stack.enter_context(patch.object(AUTOLAND, name, value))
            stack.enter_context(patch.object(AUTOLAND, "sh", side_effect=shell))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            result = AUTOLAND.run_bundle(self.slug, live=True, token_env="TEST_TOKEN")
        return result

    def test_baseline_exit_one_is_stale_premise(self):
        result = self.run_autoland(baseline_res=(1, "error[HYB-E6002]: already rejects"))
        self.assertEqual(result, 3)

    def test_baseline_exit_two_is_checker_failed(self):
        result = self.run_autoland(baseline_res=(2, "hyb: missing file or I/O error"))
        self.assertEqual(result, 8)

    def test_patched_exit_two_or_signal_is_checker_failed(self):
        result_exit2 = self.run_autoland(baseline_res=(0, ""), patched_res=(2, "error[HYB-E6002]: crash"))
        self.assertEqual(result_exit2, 8)
        result_sig = self.run_autoland(baseline_res=(0, ""), patched_res=(-15, "error[HYB-E6002]: killed"))
        self.assertEqual(result_sig, 8)

    def test_patched_longer_code_prefix_is_wrong_code(self):
        result = self.run_autoland(baseline_res=(0, ""), patched_res=(1, "error[HYB-E60020]: different error"))
        self.assertEqual(result, 6)

    def test_exact_rejection_lands_successfully(self):
        result = self.run_autoland(baseline_res=(0, ""), patched_res=(1, "error[HYB-E6002]: exact match"))
        self.assertEqual(result, 0)
        meta = json.loads((self.bundle_dir / "meta.json").read_text())
        self.assertEqual(meta["status"], "landed")
        self.assertEqual(meta["landedSha"], "1234567890abcdef")

    def test_log_event_forwards_pr_not_issue(self):
        recorded_cmd = []

        def fake_sh(cmd):
            recorded_cmd.extend(cmd)
            return subprocess.CompletedProcess(cmd, 0, "", "")

        with patch.object(AUTOLAND, "sh", side_effect=fake_sh):
            AUTOLAND.log_event("TEST_TOKEN", "autoland", "landed", "test note", issue=123, live=True)

        self.assertIn("--pr", recorded_cmd)
        self.assertIn("123", recorded_cmd)
        self.assertNotIn("--issue", recorded_cmd)


if __name__ == "__main__":
    unittest.main()
