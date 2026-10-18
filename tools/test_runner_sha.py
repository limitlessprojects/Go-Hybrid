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


if __name__ == "__main__":
    unittest.main()
