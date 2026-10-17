"""Regression tests for tools/check-versions.sh (the version-drift guard).

Each case copies the files the script reads into a temporary tree, applies
one edit, and runs the real script with --no-toolchain.
"""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
FILES = (
    "tools/check-versions.sh",
    "tools/env.sh",
    "README.md",
    "PLAN.md",
    "compiler/crates/hyb/src/main.rs",
    "compiler/crates/hyb/build.rs",
)


class CheckVersionsTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-check-versions-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in FILES:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)

    def edit(self, name, old, new):
        path = self.root / name
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text, f"fixture text missing from {name}")
        path.write_text(text.replace(old, new), encoding="utf-8")

    def run_check(self):
        return subprocess.run(
            ["bash", str(self.root / "tools/check-versions.sh"), "--no-toolchain"],
            cwd=self.root, capture_output=True, text=True, timeout=30, check=False,
        )

    def assertConsistent(self):
        result = self.run_check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("versions: CONSISTENT", result.stdout)

    def assertDrift(self, needle):
        result = self.run_check()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FAIL: " + needle, result.stdout)

    def test_current_tree_is_consistent(self):
        self.assertConsistent()

    def test_sentence_ending_period_still_matches(self):
        self.edit("README.md", "(Go 1.26.9 / Rust 1.99.0):", "(Go 1.26.9. Rust 1.99.0.)")
        self.assertConsistent()

    def test_readme_longer_go_version_is_drift(self):
        self.edit("README.md", "Go 1.26.9", "Go 1.26.91")
        self.assertDrift("README.md does not name Go 1.26.9")

    def test_readme_longer_rust_version_is_drift(self):
        self.edit("README.md", "Rust 1.99.0", "Rust 1.99.01")
        self.assertDrift("README.md does not name Rust 1.99.0")

    def test_readme_dots_are_literal(self):
        self.edit("README.md", "Go 1.26.9", "Go 1x26x9")
        self.assertDrift("README.md does not name Go 1.26.9")

    def test_bumped_pin_with_stale_longer_readme_is_drift(self):
        # Pin moves to go1.26.1 while README still says 1.26.12: a substring
        # match used to read this as consistent.
        self.edit("tools/env.sh", 'PINNED_GO="go1.26.9"', 'PINNED_GO="go1.26.1"')
        self.edit("README.md", "Go 1.26.9", "Go 1.26.12")
        self.edit("PLAN.md", "- Go: **go1.26.9**", "- Go: **go1.26.1**")
        self.edit("compiler/crates/hyb/src/main.rs", '"go1.26.9"', '"go1.26.1"')
        self.assertDrift("README.md does not name Go 1.26.1")

    def test_plan_history_mention_does_not_satisfy_environment_row(self):
        self.edit("PLAN.md", "- Go: **go1.26.9**", "- Go: **go1.26.8**")
        self.assertDrift("PLAN.md does not name go1.26.9")

    def test_plan_rust_environment_row_is_checked(self):
        self.edit("PLAN.md", "- Rust: **1.99.0**", "- Rust: **1.98.0**")
        self.assertDrift("PLAN.md does not name rust 1.99.0")

    def test_hyb_default_pin_dots_are_literal(self):
        self.edit("compiler/crates/hyb/src/main.rs", '"go1.26.9"', '"go1x26x9"')
        self.assertDrift("hyb default pin (compiler/crates/hyb/src/main.rs) != go1.26.9")

    def test_build_guard_default_pin_dots_are_literal(self):
        self.edit("compiler/crates/hyb/build.rs", '"1.99.0"', '"1x99x0"')
        self.assertDrift("hyb build.rs DEFAULT_PIN != rust 1.99.0")

    def test_build_guard_stale_default_pin_is_drift(self):
        self.edit("compiler/crates/hyb/build.rs", '"1.99.0"', '"1.98.0"')
        self.assertDrift("hyb build.rs DEFAULT_PIN != rust 1.99.0")


if __name__ == "__main__":
    unittest.main()
