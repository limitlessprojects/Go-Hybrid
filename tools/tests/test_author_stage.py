#!/usr/bin/env python3
"""Offline unit tests for tools/author_stage.py scope guard, --out path guard,
and CARGO_TARGET_DIR binary resolution."""

import os
import pathlib
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import author_stage as astage  # noqa: E402


class AuthorStageGuardsTest(unittest.TestCase):
    def test_scope_ok_rejects_deleted_out_of_scope_files_and_renames(self):
        diff_with_delete = (
            "diff --git a/compiler/crates/hy-check/src/lib.rs b/compiler/crates/hy-check/src/lib.rs\n"
            "--- a/compiler/crates/hy-check/src/lib.rs\n"
            "+++ b/compiler/crates/hy-check/src/lib.rs\n"
            "@@ -1 +1 @@\n"
            "-a\n"
            "+b\n"
            "diff --git a/README.md b/README.md\n"
            "deleted file mode 100644\n"
            "--- a/README.md\n"
            "+++ /dev/null\n"
        )
        self.assertIn("README.md", astage.diff_paths(diff_with_delete))
        self.assertFalse(astage.scope_ok(diff_with_delete))

        diff_with_rename = (
            "diff --git a/PLAN.md b/compiler/PLAN.md\n"
            "similarity index 100%\n"
            "rename from PLAN.md\n"
            "rename to compiler/PLAN.md\n"
        )
        self.assertIn("PLAN.md", astage.diff_paths(diff_with_rename))
        self.assertFalse(astage.scope_ok(diff_with_rename))

    def test_scope_ok_rejects_dotdot_traversal_and_ignores_hunk_lines(self):
        traversal_diff = (
            "--- a/compiler/../../.github/workflows/ci.yml\n"
            "+++ b/compiler/../../.github/workflows/ci.yml\n"
            "@@ -1 +1 @@\n"
            "-a\n"
            "+b\n"
        )
        self.assertFalse(astage.scope_ok(traversal_diff))

        hunk_plus_diff = (
            "diff --git a/compiler/crates/hy-check/src/lib.rs b/compiler/crates/hy-check/src/lib.rs\n"
            "--- a/compiler/crates/hy-check/src/lib.rs\n"
            "+++ b/compiler/crates/hy-check/src/lib.rs\n"
            "@@ -1,2 +1,3 @@\n"
            " context\n"
            "+++ b/README.md inside hunk\n"
        )
        self.assertEqual(
            astage.diff_paths(hunk_plus_diff),
            ["compiler/crates/hy-check/src/lib.rs"],
        )
        self.assertTrue(astage.scope_ok(hunk_plus_diff))

    def test_out_path_ok_resolves_traversal_and_relative_paths(self):
        self.assertTrue(astage.out_path_ok(astage.QUEUE / "author-42"))
        self.assertTrue(astage.out_path_ok(astage.ROOT / "target" / "custom-bundle"))
        self.assertFalse(astage.out_path_ok(astage.ROOT / ".." / "outside-bundle"))
        sibling = astage.ROOT.parent / (astage.ROOT.name + "-outside")
        self.assertFalse(astage.out_path_ok(sibling))

    def test_resolve_hyb_bin_honors_cargo_target_dir(self):
        custom = str(astage.ROOT / "custom-target")
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": custom}, clear=False):
            resolved = astage.resolve_hyb_bin()
            self.assertEqual(resolved.parent, pathlib.Path(custom) / "debug")
        env_without = {k: v for k, v in os.environ.items() if k != "CARGO_TARGET_DIR"}
        with patch.dict(os.environ, env_without, clear=True):
            resolved_default = astage.resolve_hyb_bin()
            self.assertEqual(resolved_default.parent, astage.COMPILER / "target" / "debug")


if __name__ == "__main__":
    unittest.main()
