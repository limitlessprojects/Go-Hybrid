from pathlib import Path
import shutil, subprocess, tempfile, unittest

ROOT = Path(__file__).resolve().parents[1]
FILES = ("tools/check-versions.sh", "tools/env.sh", "README.md", "PLAN.md", "compiler/crates/hyb/src/main.rs", "compiler/crates/hyb/build.rs")

class CheckVersionsCliAndPinFormatTests(unittest.TestCase):
    def setUp(self):
        t = tempfile.TemporaryDirectory()
        self.addCleanup(t.cleanup)
        self.root = Path(t.name)
        for name in FILES:
            dst = self.root / name
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / name, dst)

    def run_sh(self, *args):
        return subprocess.run(["bash", str(self.root / "tools/check-versions.sh"), *args], cwd=self.root, capture_output=True, text=True, timeout=30)

    def test_unknown_flags_and_malformed_pins_fail_closed(self):
        self.assertEqual(self.run_sh("--skip-toolchain").returncode, 1)
        self.assertEqual(self.run_sh("--no-toolchain", "extra").returncode, 1)
        env_p = self.root / "tools/env.sh"
        orig = env_p.read_text(encoding="utf-8")
        env_p.write_text(orig.replace('PINNED_GO="go1.26.9"', 'PINNED_GO="1.26.9"'), encoding="utf-8")
        self.assertEqual(self.run_sh("--no-toolchain").returncode, 1)
        env_p.write_text(orig.replace('PINNED_RUST="1.99.0"', 'PINNED_RUST="rust1.99.0"'), encoding="utf-8")
        self.assertEqual(self.run_sh("--no-toolchain").returncode, 1)

if __name__ == "__main__":
    unittest.main()
