"""Offline worker preparation regressions; Git fixtures never contact a service."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("checkout_worker", ROOT / "worker/script.py")
worker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(worker)
GIT = shutil.which("git")
SHELL = "/bin/sh" if os.path.isfile("/bin/sh") else None
if SHELL is None and GIT:
    # Git for Windows supplies a POSIX shell; no installation is required.
    candidate = Path(GIT).resolve().parents[1] / "bin/bash.exe"
    if candidate.is_file():
        SHELL = str(candidate)


@unittest.skipUnless(GIT and SHELL, "local Git and a POSIX shell are required")
class WorkerCheckoutTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="hybrid-worker-checkout-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.seed = self.root / "seed"
        self.work = self.root / "work"
        self.env = os.environ.copy()
        # Do not inherit credentials, hooks or URL rewrites from global Git config.
        self.env["GIT_CONFIG_NOSYSTEM"] = "1"
        self.env["GIT_CONFIG_GLOBAL"] = os.devnull
        self.git("init", "--quiet", str(self.seed))
        (self.seed / "fixture.txt").write_text("local fixture\n", encoding="utf-8")
        self.git("-C", str(self.seed), "add", "fixture.txt")
        self.git("-C", str(self.seed), "-c", "user.name=Fixture", "-c",
                 "user.email=fixture@example.invalid", "commit", "--quiet", "-m", "fixture")
        self.sha = self.git("-C", str(self.seed), "rev-parse", "HEAD").strip()

    def git(self, *args):
        result = subprocess.run([GIT, *args], env=self.env, capture_output=True,
                                text=True, timeout=20, check=True)
        return result.stdout

    def run_worker(self, *, missing_repo=False, sha=None, fallback_status=0,
                   runner_status=0, checkout_status=None, space_path=False,
                   clone_status=None, download_failure=False):
        if space_path:
            target = self.root / "seed repository"
            self.seed.rename(target)
            self.seed = target
        origin = self.root / "absent repository" if missing_repo else self.seed
        # Forward slashes work in the POSIX shell on Windows as well as Linux.
        job = {"job": "offline-checkout", "repo": origin.as_posix(),
               "sha": self.sha if sha is None else sha, "setup": [], "commands": []}
        calls = {"clone": 0, "checkout": 0, "fallback": 0, "runner": 0}

        def fake_sh(command, timeout=600, cwd=None):
            if command.startswith("git clone "):
                calls["clone"] += 1
                if clone_status is not None:
                    return clone_status, "simulated clone failure"
            elif "git checkout " in command:
                calls["checkout"] += 1
                if checkout_status is not None:
                    return checkout_status, "simulated checkout failure"
            elif "codeload.github.com" in command:
                calls["fallback"] += 1
                # The archive boundary is mocked: no curl, network or extraction.
                if not download_failure:
                    return fallback_status, "offline archive fixture"
                # Run the generated fallback shell, but shadow both network
                # and extraction commands with harmless shell functions.
                marker = shlex.quote((self.work / "tar-was-called").as_posix())
                command = ("curl() { return 22; }; "
                           "tar() { printf called > " + marker + "; return 0; }; "
                           + command)
            elif command.startswith("python3 repo/tools/runner/runner.py "):
                calls["runner"] += 1
                return runner_status, "offline runner fixture"
            else:
                # Hardware probes are outside the preparation contract.
                return 0, "offline environment"
            # Execute the actual generated clone/checkout shell command against
            # our local fixture. This catches a pipeline masking Git's status.
            proc = subprocess.run([SHELL, "-c", command], cwd=cwd, env=self.env,
                                  stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                  timeout=20)
            return proc.returncode, proc.stdout.decode("utf-8", "replace")

        original_cwd = os.getcwd()
        output = io.StringIO()
        try:
            with patch.object(worker, "WORK", str(self.work)), \
                 patch.object(worker, "JOB_JSON", json.dumps(job)), \
                 patch.object(worker, "sh", side_effect=fake_sh), \
                 contextlib.redirect_stdout(output):
                with self.assertRaises(SystemExit) as exited:
                    worker.main()
        finally:
            os.chdir(original_cwd)
        return exited.exception.code, calls, output.getvalue()

    def test_local_clone_with_spaces_checks_out_exact_revision(self):
        code, calls, _ = self.run_worker(space_path=True)
        self.assertEqual(code, 0)
        self.assertEqual(calls, {"clone": 1, "checkout": 1, "fallback": 0, "runner": 1})
        self.assertEqual(self.git("-C", str(self.work / "repo"), "rev-parse", "HEAD").strip(), self.sha)

    def test_unknown_revision_stops_before_runner(self):
        code, calls, output = self.run_worker(sha="0" * 40)
        self.assertNotEqual(code, 0)
        self.assertEqual(calls["runner"], 0)
        self.assertEqual(calls["fallback"], 0)
        self.assertIn("FATAL", output)

    def test_failed_clone_reaches_archive_fallback(self):
        code, calls, _ = self.run_worker(missing_repo=True)
        self.assertEqual(code, 0)
        self.assertEqual(calls, {"clone": 1, "checkout": 0, "fallback": 1, "runner": 1})

    def test_failed_archive_stops_before_runner(self):
        code, calls, output = self.run_worker(missing_repo=True, fallback_status=2)
        self.assertEqual(code, 2)
        self.assertEqual(calls["fallback"], 1)
        self.assertEqual(calls["runner"], 0)
        self.assertIn("FATAL", output)

    def test_failed_download_cannot_be_masked_by_successful_tar(self):
        code, calls, output = self.run_worker(clone_status=128, download_failure=True)
        self.assertEqual(code, 22)
        self.assertEqual(calls["fallback"], 1)
        self.assertEqual(calls["runner"], 0)
        self.assertFalse((self.work / "tar-was-called").exists())
        self.assertFalse(list(self.work.glob("hybrid-source-*")))
        self.assertIn("FATAL", output)

    def test_checkout_timeout_stops_before_runner(self):
        code, calls, _ = self.run_worker(checkout_status=124)
        self.assertEqual(code, 124)
        self.assertEqual(calls["runner"], 0)
        self.assertEqual(calls["fallback"], 0)

    def test_runner_failure_still_propagates(self):
        code, calls, _ = self.run_worker(runner_status=7)
        self.assertEqual(code, 7)
        self.assertEqual(calls["runner"], 1)

    def test_head_revision_remains_supported(self):
        code, calls, _ = self.run_worker(sha="HEAD")
        self.assertEqual(code, 0)
        self.assertEqual(calls["runner"], 1)
        self.assertEqual(self.git("-C", str(self.work / "repo"), "rev-parse", "HEAD").strip(), self.sha)


if __name__ == "__main__":
    unittest.main()
