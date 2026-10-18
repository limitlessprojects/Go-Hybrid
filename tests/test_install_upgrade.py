#!/usr/bin/env python3
"""Invalid offline releases must not replace a working installation."""

import hashlib
import os
from pathlib import Path
import platform
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class UpgradeTests(unittest.TestCase):
    def setUp(self):
        systems = {"Linux": "linux", "Darwin": "darwin"}
        arches = {"x86_64": "amd64", "aarch64": "arm64", "arm64": "arm64"}
        if platform.system() not in systems or platform.machine() not in arches:
            self.skipTest("installer supports Linux/macOS on amd64/arm64")
        if not shutil.which("sha256sum") and not shutil.which("shasum"):
            self.skipTest("installer requires a SHA-256 utility")
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-upgrade-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.install = self.root / "installed hybrid"
        self.bin_dir = self.root / "launcher bin"
        self.release = self.root / "release"
        self.release.mkdir()
        self.payload = self.root / "hybrid-v0.0.0-test"
        self.asset = (
            f"hybrid-v0.0.0-test-{systems[platform.system()]}-"
            f"{arches[platform.machine()]}.tar.gz"
        )
        shim = self.root / "offline tools"
        shim.mkdir()
        (shim / "curl").write_text(
            f"#!{sys.executable}\n"
            "import os, pathlib, shutil, sys\n"
            "args = sys.argv[1:]\n"
            "prefix = 'https://github.com/limitlessprojects/Go-Hybrid/releases/download/v0.0.0-test/'\n"
            "if len(args) != 4 or args[0] != '-fsSL' or args[2] != '-o' or not args[1].startswith(prefix):\n"
            "    sys.exit('unexpected offline download')\n"
            "name = args[1][len(prefix):]\n"
            "if name != pathlib.Path(name).name:\n"
            "    sys.exit('unexpected asset path')\n"
            "shutil.copyfile(pathlib.Path(os.environ['HYB_TEST_RELEASE_DIR']) / name, args[3])\n",
            encoding="utf-8",
        )
        (shim / "curl").chmod(0o755)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("HYB_")}
        self.env.update({
            "PATH": str(shim) + os.pathsep + os.environ.get("PATH", "/usr/bin:/bin"),
            "HYB_VERSION": "v0.0.0-test",
            "HYB_INSTALL_DIR": str(self.install),
            "HYB_BIN_DIR": str(self.bin_dir),
            "HYB_TEST_RELEASE_DIR": str(self.release),
        })

    def package(self, version, defect=None):
        if self.payload.exists():
            shutil.rmtree(self.payload)
        self.payload.mkdir()
        if defect != "missing runtime":
            (self.payload / "runtime").mkdir()
            (self.payload / "runtime" / "version").write_text(version)
        if defect != "missing CLI":
            cli = self.payload / "hyb"
            cli.write_text(
                "#!/bin/sh\n"
                + ("exit 42\n" if defect == "broken CLI" else f"printf '%s\\n' '{version}'\n"),
                encoding="utf-8",
            )
            cli.chmod(0o755)
        archive_path = self.release / self.asset
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(self.payload, arcname=self.payload.name)
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        (self.release / f"{self.asset}.sha256").write_text(f"{digest}  {self.asset}\n")

    def install_release(self):
        return subprocess.run(
            ["sh", str(ROOT / "install.sh")], env=self.env,
            capture_output=True, text=True, timeout=15,
        )

    def snapshot(self):
        return {
            str(p.relative_to(self.root)): (p.read_bytes(), p.stat().st_mode)
            for folder in (self.install, self.bin_dir)
            for p in folder.rglob("*") if p.is_file()
        }

    def test_invalid_upgrade_preserves_previous_installation(self):
        for defect in ("missing CLI", "missing runtime", "broken CLI"):
            with self.subTest(defect=defect):
                self.package("old")
                installed = self.install_release()
                self.assertEqual(installed.returncode, 0, installed.stdout + installed.stderr)
                before = self.snapshot()
                self.package("new", defect)
                failed = self.install_release()
                self.assertNotEqual(failed.returncode, 0)
                self.assertEqual(self.snapshot(), before)
                probe = subprocess.run(
                    [str(self.bin_dir / "hyb")], env=self.env,
                    capture_output=True, text=True, timeout=5,
                )
                self.assertEqual((probe.returncode, probe.stdout), (0, "old\n"))

    def test_healthy_upgrade_replaces_previous_installation(self):
        for version in ("old", "new"):
            self.package(version)
            result = self.install_release()
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual((self.install / "runtime" / "version").read_text(), version)
            probe = subprocess.run(
                [str(self.bin_dir / "hyb")], env=self.env,
                capture_output=True, text=True, timeout=5,
            )
            self.assertEqual((probe.returncode, probe.stdout), (0, version + "\n"))

    def test_launcher_cannot_overwrite_payload_through_directory_alias(self):
        self.package("old")
        installed = self.install_release()
        self.assertEqual(installed.returncode, 0, installed.stdout + installed.stderr)
        before = self.snapshot()
        alias = self.root / "payload bin alias"
        alias.symlink_to(self.install / "bin", target_is_directory=True)
        self.package("new")
        for launcher_dir in (self.install / "bin", self.install / "bin" / ".", alias):
            with self.subTest(launcher_dir=launcher_dir):
                # Kill the whole process group on regression: an exec loop
                # otherwise survives a timeout that only kills the installer.
                proc = subprocess.Popen(
                    ["sh", str(ROOT / "install.sh")],
                    env={**self.env, "HYB_BIN_DIR": str(launcher_dir)},
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                    start_new_session=True,
                )
                try:
                    stdout, stderr = proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.communicate()
                    self.fail("installer hung after launcher overwrote its payload")
                self.assertNotEqual(proc.returncode, 0, stdout + stderr)
                self.assertIn("HYB_BIN_DIR must differ", stderr)
                self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main(verbosity=2)
