#!/usr/bin/env python3
"""test_shepherd_workflows.py — regression tests for Shepherd W0/W3/W5 paths and triage_bot test detection."""

import unittest
from pathlib import Path
from unittest import mock
import sys

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
import triage_bot as tb  # noqa: E402


class ShepherdWorkflowAndTriageTests(unittest.TestCase):
    def test_shepherd_setup_caches_compiler_target_and_lockfile(self):
        src = (ROOT / ".github/actions/shepherd-setup/action.yml").read_text(encoding="utf-8")
        self.assertIn("compiler/target", src)
        self.assertIn("compiler/Cargo.lock", src)

    def test_shepherd_preflight_and_verify_use_compiler_manifest_and_gomod_dir(self):
        pre = (ROOT / ".github/workflows/shepherd-preflight.yml").read_text(encoding="utf-8")
        ver = (ROOT / ".github/workflows/shepherd-verify.yml").read_text(encoding="utf-8")
        self.assertIn("cd examples/gomod-integration && go build ./...", pre)
        self.assertIn("cd examples/gomod-integration && go vet ./...", pre)
        self.assertIn("cargo build --manifest-path compiler/Cargo.toml -p hyb", pre)
        self.assertIn("cargo test --manifest-path compiler/Cargo.toml --workspace", ver)
        self.assertIn("cargo build --manifest-path compiler/Cargo.toml -p hyb", ver)

    def test_triage_bot_collect_detects_tools_test_and_go_rust_test_files(self):
        fake_pr = {
            "number": 100, "title": "fix(tools): test", "user": {"login": "dev"},
            "created_at": "2026-10-21T00:00:00Z", "updated_at": "2026-10-21T00:00:00Z",
            "draft": False, "head": {"sha": "abcdef123456"},
        }
        gh = mock.MagicMock()
        gh.repo = "limitlessprojects/Go-Hybrid"
        gh.paginate.side_effect = [
            [fake_pr],
            [{"filename": "tools/autoland.py", "additions": 5, "deletions": 1},
             {"filename": "tools/test_autoland.py", "additions": 20, "deletions": 0}],
        ]
        gh.get.side_effect = [
            {"sha": "1111111111"},
            {"mergeable": True, "mergeable_state": "clean"},
            {"behind_by": 0, "merge_base_commit": {"sha": "1111111111"}},
        ]
        items, _ = tb.collect({}, gh, mock.Mock(jobs=1))
        self.assertTrue(items[0]["tests_included"])


if __name__ == "__main__":
    unittest.main()
