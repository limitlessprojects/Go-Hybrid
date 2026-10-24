"""Candidate cleanup against disposable Git repos; no compiler/model required."""
import contextlib
import importlib.util
import pathlib
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "specialized_cleanup_subject", pathlib.Path(__file__).with_name("specialized_brain.py")
)
brain = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(brain)


class CandidateCleanupTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        self.git("init", "-q")
        (self.root / ".gitignore").write_text("target/\ncompiler/target/\n")
        (self.root / "compiler").mkdir()
        self.source = self.root / "compiler/source.rs"
        self.source.write_text("original\n")
        self.git("add", ".")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                 "commit", "-qm", "baseline")
        self.candidate = {
            "name": "fixture", "edits": [("compiler/source.rs", "original", "candidate")],
            "new_files": [("compiler/new.rs", "candidate test\n")],
            "rationale": "fixture rationale",
        }
        family = types.SimpleNamespace(candidates=lambda: [self.candidate])
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.multiple(
            brain, ROOT=self.root, COMPILER=self.root / "compiler",
            HYB=self.root / "compiler/target/debug/hyb",
            SMOKE_DIR=self.root / "compiler/target/author-stage",
            BRAIN_TRACE=self.root / "target/author-brain-trace",
            FAMILIES={"HYB-E6001": "cleanup_fixture"},
        ))
        self.stack.enter_context(patch.dict(sys.modules, {"cleanup_fixture": family}))

    def git(self, *args):
        return subprocess.run(["git", *args], cwd=self.root, check=True,
                              capture_output=True, text=True).stdout

    def propose(self):
        return brain.propose(1, 1, "fn main() {}\n", "HYB-E6001", "")

    def assertClean(self):
        self.assertEqual(self.source.read_text(), "original\n")
        self.assertFalse((self.root / "compiler/new.rs").exists())
        self.assertEqual(self.git("status", "--porcelain"), "")

    def test_build_timeout_restores_candidate_and_new_file(self):
        real_sh = brain.sh

        def command(args, **kwargs):
            if args[0] == "cargo":
                self.assertIn("candidate", self.source.read_text())
                # A real timed-out child exercises smoke's subprocess exception.
                return real_sh([sys.executable, "-c", "import time; time.sleep(10)"],
                               timeout=0.05)
            return real_sh(args, **kwargs)

        with patch.object(brain, "sh", side_effect=command):
            with self.assertRaises(subprocess.TimeoutExpired):
                self.propose()
        self.assertClean()

    def test_missing_build_executable_restores_candidate(self):
        real_sh = brain.sh

        def command(args, **kwargs):
            if args[0] == "cargo":
                return real_sh([str(self.root / "missing-cargo")])
            return real_sh(args, **kwargs)

        with patch.object(brain, "sh", side_effect=command):
            with self.assertRaises(FileNotFoundError):
                self.propose()
        self.assertClean()

    def test_interrupt_restores_candidate(self):
        with patch.object(brain, "smoke", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.propose()
        self.assertClean()

    def test_diff_read_failure_restores_candidate(self):
        # Compiler gate is an explicit stub; diff creation and cleanup use real Git.
        with patch.object(brain, "smoke", return_value=(True, "")):
            with patch.object(brain, "diff_of", side_effect=OSError("fixture read failure")):
                with self.assertRaises(OSError):
                    self.propose()
        self.assertClean()

    def test_partial_apply_io_failure_restores_prior_edits(self):
        self.candidate["new_files"] = [("missing-parent/new.rs", "test\n")]
        with self.assertRaises(FileNotFoundError):
            self.propose()
        self.assertClean()

    def test_rejected_candidate_is_clean(self):
        with patch.object(brain, "smoke", return_value=(False, "fixture gate failure")):
            self.assertIn("No diff", self.propose())
        self.assertClean()

    def test_success_returns_patch_and_restores_tree(self):
        with patch.object(brain, "smoke", return_value=(True, "")):
            response = self.propose()
        self.assertIn("+++ b/compiler/source.rs", response)
        self.assertIn("+++ b/compiler/new.rs", response)
        self.assertIn("+candidate", response)
        self.assertClean()


if __name__ == "__main__":
    unittest.main()
