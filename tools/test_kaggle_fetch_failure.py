"""Offline CLI regressions: failed output downloads must not promote summaries.

The real job.py CLI runs against a disposable Python stand-in for Kaggle.
No Kaggle account, token, network request, or remote computation is used.
"""
import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys
import tempfile
import unittest

JOB_SOURCE = Path(__file__).with_name("job.py")
FAKE_CLI = r'''
import json
import os
from pathlib import Path
import sys

assert sys.argv[1:3] == ["kernels", "output"], sys.argv
assert sys.argv[3] == "fixture-owner/fixture-kernel", sys.argv
assert sys.argv[4] == "-p" and len(sys.argv) == 6, sys.argv
mode = os.environ["FAKE_KAGGLE_MODE"]
raw = Path(sys.argv[5])
if mode in ("fresh", "partial"):
    summary = raw / "download" / "summary.json"
    summary.parent.mkdir(parents=True, exist_ok=True)
    summary.write_text(json.dumps({"run": "fresh-fixture", "ok": True}), encoding="utf-8")
if mode in ("failure", "partial"):
    print("simulated output download failure", file=sys.stderr)
    sys.exit(7)
print("simulated output download success")
'''


@unittest.skipUnless(os.name == "posix", "job.py uses /bin/sh")
class KaggleFetchFailureTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hyfetch-", dir="/tmp")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        tools = self.root / "tools"
        tools.mkdir()
        self.script = tools / "job.py"
        shutil.copyfile(JOB_SOURCE, self.script)
        worker = self.root / "worker"
        worker.mkdir()
        (worker / "kernel-metadata.json").write_text(
            json.dumps({"id": "fixture-owner/fixture-kernel"}), encoding="utf-8")
        self.fake = self.root / "fake_kaggle.py"
        self.fake.write_text(FAKE_CLI, encoding="utf-8")
        self.jobdir = self.root / "job"
        self.raw = self.jobdir / "result" / "raw"
        self.raw.mkdir(parents=True)
        self.canonical = self.jobdir / "result" / "summary.json"

    def seed_stale_raw(self):
        stale = self.raw / "old" / "summary.json"
        stale.parent.mkdir()
        stale.write_bytes(b'{"run":"stale-fixture","ok":true}\n')
        return stale

    def run_fetch(self, mode, missing_cli=False):
        command = (str(self.root / "nonexistent-kaggle") if missing_cli else
                   shlex.join([sys.executable, "-S", str(self.fake)]))
        # A minimal environment cannot accidentally pass user tokens to a fixture.
        env = {"PATH": os.defpath, "HOME": str(self.root),
               "KAGGLE_CLI": command, "FAKE_KAGGLE_MODE": mode,
               "PYTHONUTF8": "1"}
        return subprocess.run(
            [sys.executable, "-S", str(self.script), "kaggle", "fetch", str(self.jobdir)],
            cwd=self.root, env=env, capture_output=True, text=True, timeout=10)

    def assert_download_failed(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("kaggle kernels output failed", result.stderr)
        self.assertNotIn("summary.json ->", result.stdout)

    def test_failed_download_without_cache_has_nonzero_cli_status(self):
        result = self.run_fetch("failure")
        self.assert_download_failed(result)
        self.assertIn("exit 7", result.stderr)
        self.assertIn("simulated output download failure", result.stdout)
        self.assertFalse(self.canonical.exists())

    def test_failed_download_does_not_overwrite_existing_summary(self):
        stale = self.seed_stale_raw()
        previous = b'{"run":"previous-canonical","preserve":true}\n'
        self.canonical.write_bytes(previous)
        result = self.run_fetch("failure")
        self.assertEqual(self.canonical.read_bytes(), previous)
        self.assertEqual(stale.read_bytes(), b'{"run":"stale-fixture","ok":true}\n')
        self.assert_download_failed(result)

    def test_failed_download_does_not_create_summary_from_old_raw(self):
        self.seed_stale_raw()
        result = self.run_fetch("failure")
        self.assertFalse(self.canonical.exists(), result.stdout + result.stderr)
        self.assert_download_failed(result)

    def test_partial_download_is_not_promoted_after_nonzero_exit(self):
        result = self.run_fetch("partial")
        self.assertTrue((self.raw / "download" / "summary.json").exists())
        self.assertFalse(self.canonical.exists(), result.stdout + result.stderr)
        self.assert_download_failed(result)

    def test_missing_cli_fails_instead_of_reusing_old_raw(self):
        self.seed_stale_raw()
        result = self.run_fetch("unused", missing_cli=True)
        self.assertFalse(self.canonical.exists(), result.stdout + result.stderr)
        self.assert_download_failed(result)
        self.assertIn("exit 127", result.stderr)

    def test_successful_download_keeps_nested_summary_copy(self):
        self.canonical.write_bytes(b'{"run":"previous-canonical"}\n')
        result = self.run_fetch("fresh")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads(self.canonical.read_text(encoding="utf-8")),
                         {"run": "fresh-fixture", "ok": True})
        self.assertIn("summary.json ->", result.stdout)
        self.assertEqual(result.stderr, "")

    def test_success_without_summary_retains_existing_warning_behavior(self):
        result = self.run_fetch("empty")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("WARNING: no summary.json in output", result.stdout)
        self.assertFalse(self.canonical.exists())


if __name__ == "__main__":
    unittest.main()
