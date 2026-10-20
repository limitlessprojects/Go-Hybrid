#!/usr/bin/env python3
"""Run the real installer against offline archives; never contact GitHub."""

import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SHELL = shutil.which("sh")
if os.name == "nt":
    candidate = Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/bin/bash.exe"
    SHELL = str(candidate) if candidate.exists() else None


def shell_path(path):
    path = Path(path).resolve()
    if os.name == "nt":
        return "/" + path.drive[0].lower() + path.as_posix()[2:]
    return str(path)


@unittest.skipUnless(SHELL, "requires a POSIX shell (Git Bash works on Windows)")
class InstallerChecksumTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="checksum-test-", dir=ROOT)
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.assertTrue(self.root.is_relative_to(ROOT))
        self.install = self.root / "installed hybrid"
        self.bin_dir = self.root / "launcher bin"
        self.executed = self.root / "executed"
        shims = self.root / "offline tools"
        shims.mkdir()
        self.asset = "hybrid-v0.0.0-test-linux-amd64.tar.gz"
        archive_path = self.root / self.asset
        with tarfile.open(archive_path, "w:gz") as archive:
            for name, contents, mode in (
                ("hyb", b'#!/bin/sh\nprintf "executed\\n" >> "$HYB_TEST_EXECUTED"\nexit 0\n', 0o755),
                ("runtime/fixture.txt", b"offline runtime\n", 0o644),
            ):
                info = tarfile.TarInfo("hybrid-v0.0.0-test/" + name)
                info.size, info.mode = len(contents), mode
                archive.addfile(info, io.BytesIO(contents))
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        self.checksum = self.root / (self.asset + ".sha256")
        self.checksum.write_text(f"{digest}  {self.asset}\n", encoding="utf-8", newline="\n")
        (shims / "uname").write_text(
            '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; *) exit 2;; esac\n',
            encoding="utf-8", newline="\n",
        )
        (shims / "curl").write_text(
            '#!/bin/sh\n'
            '[ "$#" -eq 4 ] && [ "$1" = "-fsSL" ] && [ "$3" = "-o" ] || exit 2\n'
            'prefix="https://github.com/limitlessprojects/Go-Hybrid/releases/download/v0.0.0-test/"\n'
            'case "$2" in\n'
            '  "${prefix}${HYB_TEST_ASSET}")\n'
            '    [ "${HYB_TEST_FAIL_ARCHIVE:-0}" = 0 ] || exit 22\n'
            '    cp "$HYB_TEST_ARCHIVE" "$4";;\n'
            '  "${prefix}${HYB_TEST_ASSET}.sha256")\n'
            '    [ "${HYB_TEST_FAIL_CHECKSUM:-0}" = 0 ] || exit 22\n'
            '    cp "$HYB_TEST_CHECKSUM" "$4";;\n'
            '  *) exit 2;;\n'
            'esac\n',
            encoding="utf-8", newline="\n",
        )
        for shim in shims.iterdir():
            shim.chmod(0o755)
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("HYB_")}
        self.env.update({
            "HYB_VERSION": "v0.0.0-test",
            "HYB_INSTALL_DIR": shell_path(self.install),
            "HYB_BIN_DIR": shell_path(self.bin_dir),
            "HYB_TEST_INSTALLER": shell_path(ROOT / "install.sh"),
            "HYB_TEST_SHIMS": shell_path(shims),
            "HYB_TEST_ARCHIVE": shell_path(archive_path),
            "HYB_TEST_CHECKSUM": shell_path(self.checksum),
            "HYB_TEST_ASSET": self.asset,
            "HYB_TEST_EXECUTED": shell_path(self.executed),
        })

    def run_installer(self, **overrides):
        return subprocess.run(
            [SHELL, "-c", 'PATH="$HYB_TEST_SHIMS:$PATH"; export PATH; exec sh "$HYB_TEST_INSTALLER"'],
            env={**self.env, **overrides}, capture_output=True, text=True, encoding="utf-8", timeout=20,
        )

    def assert_rejected_without_installing(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(self.install.exists(), result.stdout + result.stderr)
        self.assertFalse(self.executed.exists(), "an unverified executable ran")

    def test_valid_checksum_installs_and_runs_smoke_test(self):
        result = self.run_installer()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("sha256 verified", result.stdout)
        self.assertTrue((self.install / "runtime/fixture.txt").exists())
        self.assertTrue(self.executed.exists())

    def test_checksum_download_failure_aborts_before_installation(self):
        self.assert_rejected_without_installing(self.run_installer(HYB_TEST_FAIL_CHECKSUM="1"))

    def test_checksum_download_failure_preserves_existing_installation(self):
        old_binary = self.install / "bin/hyb"
        old_binary.parent.mkdir(parents=True)
        old_binary.write_text("previous binary", encoding="utf-8")
        runtime = self.install / "runtime/previous.txt"
        runtime.parent.mkdir()
        runtime.write_text("previous runtime", encoding="utf-8")
        result = self.run_installer(HYB_TEST_FAIL_CHECKSUM="1")
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(old_binary.read_text(encoding="utf-8"), "previous binary")
        self.assertEqual(runtime.read_text(encoding="utf-8"), "previous runtime")
        self.assertFalse(self.executed.exists())

    def test_mismatched_checksum_aborts(self):
        self.checksum.write_text(f"{'0' * 64}  {self.asset}\n", encoding="utf-8", newline="\n")
        self.assert_rejected_without_installing(self.run_installer())

    def test_empty_checksum_aborts(self):
        self.checksum.write_text("", encoding="utf-8")
        self.assert_rejected_without_installing(self.run_installer())

    def test_checksum_without_archive_record_aborts(self):
        for manifest in (" \n\t\n", "not a checksum\n",
                         f"{'0' * 64}  another-archive.tar.gz\n"):
            with self.subTest(manifest=manifest):
                self.checksum.write_text(manifest, encoding="utf-8")
                self.assert_rejected_without_installing(self.run_installer())

    def test_archive_download_failure_aborts(self):
        self.assert_rejected_without_installing(self.run_installer(HYB_TEST_FAIL_ARCHIVE="1"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
