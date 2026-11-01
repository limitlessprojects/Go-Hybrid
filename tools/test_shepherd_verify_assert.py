"""Offline CLI checks for the Shepherd landing assertion; Git is a local shim."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SHEPHERD = Path(__file__).with_name("shepherd.py")
HEAD = "a" * 40
MAIN = "b" * 40


class VerifyAssertTests(unittest.TestCase):
    def run_assert(self, fetch_exit=0, ancestor_exit=0, remote_exit=0):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            calls = root / "calls.jsonl"
            git = root / "git"
            git.write_text(
                "#!" + sys.executable + "\n"
                "import json, os, sys\n"
                "with open(os.environ['GIT_SHIM_CALLS'], 'a') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n"
                "if 'ls-remote' in sys.argv:\n"
                "    print('" + MAIN + "\\trefs/heads/main')\n"
                "    sys.exit(int(os.environ['GIT_SHIM_REMOTE_EXIT']))\n"
                "if 'fetch' in sys.argv:\n"
                "    code = int(os.environ['GIT_SHIM_FETCH_EXIT'])\n"
                "    if code: print('simulated fetch failure', file=sys.stderr)\n"
                "    sys.exit(code)\n"
                "if 'merge-base' in sys.argv:\n"
                "    sys.exit(int(os.environ['GIT_SHIM_ANCESTOR_EXIT']))\n"
                "sys.exit(99)\n",
                encoding="utf-8",
            )
            git.chmod(0o755)
            env = dict(os.environ, PATH=str(root), GIT_SHIM_CALLS=str(calls),
                       GIT_SHIM_FETCH_EXIT=str(fetch_exit),
                       GIT_SHIM_ANCESTOR_EXIT=str(ancestor_exit),
                       GIT_SHIM_REMOTE_EXIT=str(remote_exit),
                       GITHUB_STEP_SUMMARY=str(root / "summary.md"))
            result = subprocess.run(
                [sys.executable, str(SHEPHERD), "verify-assert", "--head", HEAD,
                 "--repo", str(root)], capture_output=True, text=True, env=env,
                timeout=10,
            )
            commands = [json.loads(line) for line in calls.read_text().splitlines()]
            return result, commands

    def test_failed_fetch_cannot_confirm_stale_fetch_head(self):
        # The stale ancestry result would be successful if queried. A failed
        # fetch must stop before that result can certify the current remote.
        result, commands = self.run_assert(fetch_exit=128, ancestor_exit=0)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("ASSERT-FAIL", result.stdout)
        self.assertIn("simulated fetch failure", result.stdout)
        self.assertNotIn("LAND-CONFIRMED", result.stdout)
        self.assertFalse(any("merge-base" in command for command in commands))

    def test_successful_fetch_and_ancestry_confirm_landing(self):
        result, commands = self.run_assert()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("LAND-CONFIRMED", result.stdout)
        self.assertTrue(any("merge-base" in command for command in commands))

    def test_successful_fetch_without_ancestry_rejects_landing(self):
        result, _ = self.run_assert(ancestor_exit=1)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("NOT-LANDED", result.stdout)
        self.assertNotIn("LAND-CONFIRMED", result.stdout)

    def test_failed_remote_lookup_stops_before_fetch(self):
        result, commands = self.run_assert(remote_exit=128)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("ASSERT-FAIL", result.stdout)
        self.assertFalse(any("fetch" in command for command in commands))


if __name__ == "__main__":
    unittest.main()
