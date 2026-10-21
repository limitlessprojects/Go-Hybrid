"""Regression tests for CLI flag validation and pin format guards in tools/check-versions.sh."""
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


class CheckVersionsCliAndPinFormatTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-check-versions-cli-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in FILES:
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, target)

    def run_script(self, *args):
        return subprocess.run(
            ["bash", str(self.root / "tools/check-versions.sh"), *args],
            cwd=self.root,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )

    def test_unknown_flag_fails_closed(self):
        res = self.run_script("--skip-toolchain")
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: unsupported argument(s): --skip-toolchain", res.stdout)

    def test_extra_positional_argument_after_no_toolchain_fails_closed(self):
        res = self.run_script("--no-toolchain", "extra")
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: unsupported argument(s)", res.stdout)

    def test_malformed_pinned_go_without_go_prefix_fails_closed(self):
        env_path = self.root / "tools/env.sh"
        env_path.write_text(
            env_path.read_text(encoding="utf-8").replace('PINNED_GO="go1.26.9"', 'PINNED_GO="1.26.9"'),
            encoding="utf-8",
        )
        res = self.run_script("--no-toolchain")
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: malformed PINNED_GO in tools/env.sh", res.stdout)

    def test_malformed_pinned_rust_fails_closed(self):
        env_path = self.root / "tools/env.sh"
        env_path.write_text(
            env_path.read_text(encoding="utf-8").replace('PINNED_RUST="1.99.0"', 'PINNED_RUST="rust1.99.0"'),
            encoding="utf-8",
        )
        res = self.run_script("--no-toolchain")
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: malformed PINNED_RUST in tools/env.sh", res.stdout)


if __name__ == "__main__":
    unittest.main()
