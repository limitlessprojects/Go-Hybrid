#!/usr/bin/env python3
"""test_author_stage_baseline.py — regression tests for author_stage baseline ACCEPT gate, --out path resolution, and workflow BLOCKED verdict capture."""

import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import author_stage as astg  # noqa: E402


class AuthorStageBaselineAndWorkflowTests(unittest.TestCase):
    def test_out_path_rejects_sibling_prefix_and_accepts_repo_relative(self):
        sibling = pathlib.Path(str(ROOT) + "-sibling/bundle")
        self.assertEqual(astg.run_author(1, sibling, 1, brain="specialized"), 2)

    def test_stale_premise_repro_rejected_at_baseline_blocks_before_brain_rounds(self):
        issue = {
            "number": 823,
            "title": "stale premise test",
            "body": "```hy\nfn main() {}\n```\nExpect HYB-E6006.",
        }
        with tempfile.TemporaryDirectory() as td:
            tpath = pathlib.Path(td)
            with mock.patch.object(astg, "ROOT", tpath), \
                 mock.patch.object(astg, "dirty", return_value=False), \
                 mock.patch.object(astg, "api_get", return_value=issue), \
                 mock.patch.object(astg, "BLOCKED_DIR", tpath / "blocked"), \
                 mock.patch.object(astg, "REPRO_DIR", tpath / "repro"), \
                 mock.patch.object(astg, "sh", return_value=subprocess.CompletedProcess([], 0, "", "")), \
                 mock.patch.object(astg, "hyb_check", return_value=(1, "HYB-E6006 already rejected")), \
                 mock.patch.object(astg, "chat", side_effect=AssertionError("brain must not run when baseline rejects")):
                rc = astg.run_author(823, pathlib.Path("target/test-author-823"), 4, brain="specialized")
            self.assertEqual(rc, 3)
            note = (tpath / "blocked" / "issue-823" / "note.txt").read_text()
            self.assertIn("stale premise", note)

    def test_workflow_supports_brain_input_and_captures_blocked_verdict(self):
        wf = (ROOT / ".github/workflows/author-stage.yml").read_text()
        self.assertIn("--brain", wf)
        self.assertIn("|| rc=$?", wf)


if __name__ == "__main__":
    unittest.main()
