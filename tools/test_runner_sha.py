"""Unit tests for tools/runner/runner.py commit PIN SHA validation (Issue #72)."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class TestRunnerShaValidation(unittest.TestCase):
    def setUp(self):
        self.repo_dir = Path(__file__).resolve().parent.parent
        self.runner_py = self.repo_dir / "tools" / "runner" / "runner.py"

    def test_sha_mismatch_skips_setup_and_commands(self):
        with tempfile.TemporaryDirectory(prefix="test-sha-mismatch-") as td:
            root = Path(td)
            setup_marker = root / "setup.ran"
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-mismatch",
                "sha": "0" * 40,
                "tier": "local",
                "setup": [{"name": "s1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{setup_marker}').write_text('1')"]}],
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(self.repo_dir), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 1)
            self.assertFalse(setup_marker.exists())
            self.assertFalse(cmd_marker.exists())

            summary_file = results_dir / "summary.json"
            self.assertTrue(summary_file.exists())
            summary = json.loads(summary_file.read_text(encoding="utf-8"))
            self.assertFalse(summary["ok"])
            steps = summary.get("steps", [])
            self.assertEqual(len(steps), 1)
            self.assertEqual(steps[0]["name"], "sha-check")
            self.assertEqual(steps[0]["exit"], 1)

    def test_sha_match_head_runs_setup_and_commands(self):
        with tempfile.TemporaryDirectory(prefix="test-sha-head-") as td:
            root = Path(td)
            setup_marker = root / "setup.ran"
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-head",
                "sha": "HEAD",
                "tier": "local",
                "setup": [{"name": "s1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{setup_marker}').write_text('1')"]}],
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(self.repo_dir), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 0)
            self.assertTrue(setup_marker.exists())
            self.assertTrue(cmd_marker.exists())

            summary_file = results_dir / "summary.json"
            self.assertTrue(summary_file.exists())
            summary = json.loads(summary_file.read_text(encoding="utf-8"))
            self.assertTrue(summary["ok"])
            steps = summary.get("steps", [])
            self.assertEqual([s["name"] for s in steps], ["sha-check", "s1", "c1"])


    def test_head_lookup_failure_skips_setup_and_commands(self):
        with tempfile.TemporaryDirectory(prefix="test-sha-fail-") as td:
            root = Path(td)
            dummy_repo = root / "corrupt-repo"
            dummy_repo.mkdir()
            (dummy_repo / ".git").mkdir()  # empty .git dir, git rev-parse HEAD fails
            setup_marker = root / "setup.ran"
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-fail",
                "sha": "HEAD",
                "tier": "local",
                "setup": [{"name": "s1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{setup_marker}').write_text('1')"]}],
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(dummy_repo), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 1)
            self.assertFalse(setup_marker.exists())
            self.assertFalse(cmd_marker.exists())

            summary_file = results_dir / "summary.json"
            self.assertTrue(summary_file.exists())
            summary = json.loads(summary_file.read_text(encoding="utf-8"))
            self.assertFalse(summary["ok"])
            steps = summary.get("steps", [])
            self.assertEqual(len(steps), 1)
            self.assertEqual(steps[0]["name"], "sha-check")
            self.assertEqual(steps[0]["exit"], 1)
            self.assertIn("failed to resolve HEAD", steps[0]["out_tail"])

    def test_pinned_sha_without_git_is_not_reported_green(self):
        # The Kaggle worker can fall back to a codeload tarball, which has no
        # .git. A job pinned to a real commit must not run (or report ok) when
        # the pin cannot be verified against anything.
        with tempfile.TemporaryDirectory(prefix="test-sha-nogit-") as td:
            root = Path(td)
            tarball_repo = root / "repo"
            tarball_repo.mkdir()  # extracted sources, deliberately no .git
            self.assertFalse((tarball_repo / ".git").exists())
            setup_marker = root / "setup.ran"
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-nogit",
                "sha": "0" * 40,
                "tier": "local",
                "setup": [{"name": "s1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{setup_marker}').write_text('1')"]}],
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(tarball_repo), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 1)
            self.assertFalse(setup_marker.exists())
            self.assertFalse(cmd_marker.exists())

            summary_file = results_dir / "summary.json"
            self.assertTrue(summary_file.exists())
            summary = json.loads(summary_file.read_text(encoding="utf-8"))
            self.assertFalse(summary["ok"])
            steps = summary.get("steps", [])
            self.assertEqual(len(steps), 1)
            self.assertEqual(steps[0]["name"], "sha-check")
            self.assertEqual(steps[0]["exit"], 1)
            self.assertIn("cannot verify pinned sha", steps[0]["out_tail"])

    def test_head_pin_without_git_still_runs(self):
        # A HEAD-pinned job has no commit to verify, so the tarball fallback
        # must stay usable for it.
        with tempfile.TemporaryDirectory(prefix="test-sha-nogit-head-") as td:
            root = Path(td)
            tarball_repo = root / "repo"
            tarball_repo.mkdir()  # no .git
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-nogit-head",
                "sha": "HEAD",
                "tier": "local",
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(tarball_repo), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 0)
            self.assertTrue(cmd_marker.exists())
            summary = json.loads((results_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertTrue(summary["ok"])

    def test_gitfile_worktree_pinned_sha_and_head_pin_succeed(self):
        # In `git worktree` and submodule checkouts, `.git` is a regular file
        # (`gitdir: <path>`) rather than a directory. Both exact-SHA pins and
        # `HEAD` pins must still run `sha-check` against `git rev-parse HEAD`.
        with tempfile.TemporaryDirectory(prefix="test-sha-gitfile-") as td:
            root = Path(td)
            real_git_dir = subprocess.check_output(
                ["git", "rev-parse", "--git-dir"],
                cwd=str(self.repo_dir),
                text=True,
            ).strip()
            real_git_path = (self.repo_dir / real_git_dir).resolve()
            actual_head = subprocess.check_output(
                ["git", "rev-parse", "HEAD"],
                cwd=str(self.repo_dir),
                text=True,
            ).strip()

            worktree_repo = root / "worktree-repo"
            worktree_repo.mkdir()
            gitfile = worktree_repo / ".git"
            gitfile.write_text(f"gitdir: {real_git_path}\n", encoding="utf-8")
            self.assertTrue(gitfile.is_file())
            self.assertFalse(gitfile.is_dir())

            # 1. Exact commit SHA pin against a gitfile worktree checkout succeeds.
            cmd_marker = root / "cmd-pinned.ran"
            job_pinned = {
                "job": "test-sha-gitfile-pinned",
                "sha": actual_head,
                "tier": "local",
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_pinned_file = root / "job-pinned.json"
            job_pinned_file.write_text(json.dumps(job_pinned), encoding="utf-8")
            results_pinned = root / "results-pinned"

            res_pinned = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_pinned_file), "--repo-dir", str(worktree_repo), "--results-dir", str(results_pinned)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res_pinned.returncode, 0, res_pinned.stdout + res_pinned.stderr)
            self.assertTrue(cmd_marker.exists())
            summary_pinned = json.loads((results_pinned / "summary.json").read_text(encoding="utf-8"))
            self.assertTrue(summary_pinned["ok"])
            self.assertEqual([s["name"] for s in summary_pinned["steps"]], ["sha-check", "c1"])

            # 2. HEAD pin against a gitfile worktree checkout records the sha-check step.
            cmd_head_marker = root / "cmd-head.ran"
            job_head = {
                "job": "test-sha-gitfile-head",
                "sha": "HEAD",
                "tier": "local",
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_head_marker}').write_text('1')"]}],
            }
            job_head_file = root / "job-head.json"
            job_head_file.write_text(json.dumps(job_head), encoding="utf-8")
            results_head = root / "results-head"

            res_head = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_head_file), "--repo-dir", str(worktree_repo), "--results-dir", str(results_head)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res_head.returncode, 0, res_head.stdout + res_head.stderr)
            self.assertTrue(cmd_head_marker.exists())
            summary_head = json.loads((results_head / "summary.json").read_text(encoding="utf-8"))
            self.assertTrue(summary_head["ok"])
            self.assertEqual([s["name"] for s in summary_head["steps"]], ["sha-check", "c1"])

    def test_gitfile_worktree_corrupt_gitdir_fails_closed(self):
        # A broken `.git` gitfile (pointing to a missing/corrupt gitdir) must
        # fail closed on `git rev-parse HEAD` rather than being treated as a
        # `.git`-less tarball checkout.
        with tempfile.TemporaryDirectory(prefix="test-sha-gitfile-corrupt-") as td:
            root = Path(td)
            corrupt_worktree = root / "corrupt-worktree"
            corrupt_worktree.mkdir()
            (corrupt_worktree / ".git").write_text(f"gitdir: {root / 'missing-gitdir'}\n", encoding="utf-8")
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-gitfile-corrupt",
                "sha": "HEAD",
                "tier": "local",
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(corrupt_worktree), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 1)
            self.assertFalse(cmd_marker.exists())
            summary = json.loads((results_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertFalse(summary["ok"])
            steps = summary.get("steps", [])
            self.assertEqual(len(steps), 1)
            self.assertEqual(steps[0]["name"], "sha-check")
            self.assertEqual(steps[0]["exit"], 1)
            self.assertIn("failed to resolve HEAD", steps[0]["out_tail"])

    def test_corrupt_git_dir_inside_parent_repo_does_not_leak_parent_head(self):
        # An empty/corrupt `.git` directory inside an ancestor git repository
        # must fail closed rather than walking up parent directories and
        # resolving the ancestor repository's HEAD.
        with tempfile.TemporaryDirectory(prefix="test-sha-ceiling-", dir=str(self.repo_dir)) as td:
            root = Path(td)
            corrupt_repo = root / "nested-repo"
            (corrupt_repo / ".git").mkdir(parents=True)
            cmd_marker = root / "cmd.ran"
            job = {
                "job": "test-sha-ceiling-corrupt",
                "sha": "HEAD",
                "tier": "local",
                "commands": [{"name": "c1", "run": [sys.executable, "-c", f"from pathlib import Path; Path(r'{cmd_marker}').write_text('1')"]}],
            }
            job_file = root / "job.json"
            job_file.write_text(json.dumps(job), encoding="utf-8")
            results_dir = root / "results"

            res = subprocess.run(
                [sys.executable, str(self.runner_py), str(job_file), "--repo-dir", str(corrupt_repo), "--results-dir", str(results_dir)],
                capture_output=True,
                text=True,
            )
            self.assertEqual(res.returncode, 1)
            self.assertFalse(cmd_marker.exists())
            summary = json.loads((results_dir / "summary.json").read_text(encoding="utf-8"))
            self.assertFalse(summary["ok"])
            self.assertEqual([s["name"] for s in summary["steps"]], ["sha-check"])
            self.assertEqual(summary["steps"][0]["exit"], 1)


    def test_author_stage_gh_token_and_scope_guard(self):
        from tools import author_stage
        self.assertFalse(author_stage.scope_ok("--- a/README.md\n+++ /dev/null\n+++ b/compiler/x.rs\n"))
        self.assertFalse(author_stage.scope_ok("+++ b/compiler/../tools/autoland.py\n"))


if __name__ == "__main__":
    unittest.main()

