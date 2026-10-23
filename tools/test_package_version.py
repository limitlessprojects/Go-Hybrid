"""Reject unsafe release path components before builds or filesystem writes."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ReleaseVersionTests(unittest.TestCase):
    def run_version(self, version):
        with tempfile.TemporaryDirectory(prefix="hybrid-version-test-") as tmp:
            root = Path(tmp)
            (root / "tools").mkdir()
            (root / "runtime/rust/hyrt").mkdir(parents=True)
            shutil.copy2(ROOT / "tools/package_release.sh", root / "tools/package_release.sh")
            commands = root / "commands"
            commands.mkdir()
            sentinel = root / "cargo-called"
            (commands / "cargo").write_text(
                '#!/bin/sh\n: > "$BUILD_SENTINEL"\nexit 89\n', encoding="utf-8"
            )
            (commands / "go").write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            for command in commands.iterdir():
                command.chmod(0o755)
            result = subprocess.run(
                ["sh", str(root / "tools/package_release.sh"), version],
                env={**os.environ, "PATH": f"{commands}:{os.environ['PATH']}",
                     "BUILD_SENTINEL": str(sentinel)},
                capture_output=True, text=True, timeout=5,
            )
            return result, sentinel.exists(), (root / "dist").exists()

    def test_unsafe_version_is_rejected_before_build(self):
        for version in ("../outside", "v1/../../outside", "/tmp/release",
                        "v1\\outside", "v1 with spaces", "v1\nsecond-line", "", ".", "..", "-rf", "v1..2"):
            with self.subTest(version=version):
                result, built, dist_created = self.run_version(version)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(built, result.stdout + result.stderr)
                self.assertFalse(dist_created)
                if version:
                    self.assertIn("invalid version", result.stderr)

    def test_filename_safe_versions_reach_build_unchanged(self):
        for version in ("v0.1.0", "0.1.0", "v1.2.3-rc.1+build.5", "release_2026-10-15"):
            with self.subTest(version=version):
                result, built, _ = self.run_version(version)
                self.assertEqual(result.returncode, 89, result.stdout + result.stderr)
                self.assertTrue(built)
                self.assertIn(f"hybrid-{version}-", result.stdout)


if __name__ == "__main__":
    unittest.main()
