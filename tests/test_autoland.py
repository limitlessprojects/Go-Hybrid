#!/usr/bin/env python3
"""test_autoland.py — offline unit tests for tools/autoland.py."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import autoland as al  # noqa: E402


class AutolandTests(unittest.TestCase):
    def test_build_passes_compiler_cwd(self):
        with mock.patch.object(
            al, "sh", return_value=subprocess.CompletedProcess([], 0, "", "")
        ) as m_sh:
            al.build()
        m_sh.assert_called_once_with(
            ["cargo", "build", "-p", "hyb", "--quiet"], cwd=str(al.COMPILER)
        )

    def test_battery_workspace_step_passes_compiler_cwd(self):
        calls = []

        def fake_sh(cmd, cwd=None, **kw):
            calls.append((cmd, cwd))
            return subprocess.CompletedProcess(cmd, 0, "", "")

        with mock.patch.object(al, "sh", side_effect=fake_sh):
            res = al.battery()
        self.assertTrue(all(ok for _, ok, _ in res))
        self.assertEqual(
            calls[0],
            (["cargo", "test", "--workspace"], str(al.COMPILER)),
        )

    def test_log_event_uses_shepherd_pr_and_sha_flags(self):
        with mock.patch.object(
            al, "sh", return_value=subprocess.CompletedProcess([], 0, "", "")
        ) as m_sh:
            al.log_event("GITHUB_TOKEN", "autoland", "landed", "ok", issue=998, live=True, sha="abc1234")
        cmd = m_sh.call_args[0][0]
        self.assertIn("--pr", cmd)
        self.assertNotIn("--issue", cmd)
        self.assertEqual(cmd[cmd.index("--pr") + 1], "998")
        self.assertEqual(cmd[cmd.index("--sha") + 1], "abc1234")

    def test_valid_slug_rejects_traversal_and_separators(self):
        for bad in ("", ".", "..", "../etc", "a/b", "a\\b"):
            self.assertFalse(al.valid_slug(bad), bad)
        self.assertTrue(al.valid_slug("issue-998"))

    def test_run_bundle_rejects_missing_repro_or_patch_and_reverts_on_ineffective_patch(self):
        with tempfile.TemporaryDirectory() as td:
            q = Path(td)
            b = q / "b1"
            b.mkdir()
            (b / "meta.json").write_text(json.dumps({"issue": 10, "expected_code": "HYB-E3003"}))
            with mock.patch.object(al, "QUEUE", q):
                self.assertEqual(al.run_bundle("b1", live=False, token_env="GITHUB_TOKEN"), 2)
                (b / "repro.hy").write_text("package main\nfn main() {}\n")
                self.assertEqual(al.run_bundle("b1", live=True, token_env="GITHUB_TOKEN"), 2)
                (b / "fix.patch").write_text("diff")
                with mock.patch.object(al, "build"), \
                     mock.patch.object(al, "check_repro", side_effect=[(0, ""), (0, "")]), \
                     mock.patch.object(al, "sh", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                     mock.patch.object(al, "revert") as m_rev, \
                     mock.patch.object(al, "log_event"):
                    rc = al.run_bundle("b1", live=True, token_env="GITHUB_TOKEN")
                self.assertEqual(rc, 5)
                m_rev.assert_called()


if __name__ == "__main__":
    unittest.main()
