#!/usr/bin/env python3
"""Resolve the latest release from the redirect without truncating build metadata.

Reproduces #3778: install.sh parsed the releases/latest Location header with a
sed character class that stopped at the first '+', so a tag like
v1.2.3+build.7 resolved to v1.2.3 — the installer then downloaded and
checksum-verified the wrong (older) archive whose name and download path
matched the truncated tag. The same truncation also stripped the archive name.

Self-contained like the sibling installer suites: a throwaway release archive
per scenario, curl stubbed for the HEAD redirect, the API fallback, and the
two asset fetches — GitHub is never contacted.
"""

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
REPO = "limitlessprojects/Go-Hybrid"
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
class InstallerReleaseTagTests(unittest.TestCase):
    def build_scenario(self, tag, *, via_redirect=True):
        """Create an offline release fixture for `tag`; return its env vars.

        The curl shim answers the releases/latest HEAD with a Location header
        for `tag` (when via_redirect), or fails it so the installer falls back
        to the API, which reports `tag` as tag_name. Only assets named after
        the FULL tag are served: a truncated resolution misses the fixture and
        the installer aborts, which is exactly the #3778 failure mode.
        """
        temporary = tempfile.TemporaryDirectory(prefix="release-tag-test-", dir=ROOT)
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve()
        self.assertTrue(root.is_relative_to(ROOT))
        install = root / "installed hybrid"
        bin_dir = root / "launcher bin"
        executed = root / "executed"
        shims = root / "offline tools"
        shims.mkdir()
        asset = f"hybrid-{tag}-linux-amd64.tar.gz"
        archive_path = root / asset
        with tarfile.open(archive_path, "w:gz") as archive:
            for name, contents, mode in (
                ("hyb", b'#!/bin/sh\nprintf "executed\\n" >> "$HYB_TEST_EXECUTED"\nexit 0\n', 0o755),
                ("runtime/fixture.txt", b"offline runtime\n", 0o644),
            ):
                info = tarfile.TarInfo(f"hybrid-{tag}/" + name)
                info.size, info.mode = len(contents), mode
                archive.addfile(info, io.BytesIO(contents))
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        checksum = root / (asset + ".sha256")
        checksum.write_text(f"{digest}  {asset}\n", encoding="utf-8", newline="\n")
        (shims / "uname").write_text(
            '#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; *) exit 2;; esac\n',
            encoding="utf-8", newline="\n",
        )
        (shims / "curl").write_text(
            '#!/bin/sh\n'
            'if [ "$1" = "-fsSI" ]; then\n'
            '  [ "$2" = "https://github.com/' + REPO + '/releases/latest" ] || exit 22\n'
            '  [ -n "$HYB_TEST_REDIRECT_TAG" ] || exit 22\n'
            '  printf \'HTTP/2 302\\r\\nlocation: https://github.com/' + REPO + '/releases/tag/%s\\r\\n\\r\\n\' "$HYB_TEST_REDIRECT_TAG"\n'
            '  exit 0\n'
            'fi\n'
            '[ "$#" -eq 4 ] && [ "$1" = "-fsSL" ] && [ "$3" = "-o" ] || exit 2\n'
            'case "$2" in\n'
            '  "https://api.github.com/repos/' + REPO + '/releases/latest")\n'
            '    [ -n "$HYB_TEST_API_TAG" ] || exit 22\n'
            '    printf \'{"tag_name": "%s"}\\n\' "$HYB_TEST_API_TAG" > "$4"\n'
            '    exit 0;;\n'
            '  "https://github.com/' + REPO + '/releases/download/${HYB_TEST_TAG}/${HYB_TEST_ASSET}")\n'
            '    cp "$HYB_TEST_ARCHIVE" "$4";;\n'
            '  "https://github.com/' + REPO + '/releases/download/${HYB_TEST_TAG}/${HYB_TEST_ASSET}.sha256")\n'
            '    cp "$HYB_TEST_CHECKSUM" "$4";;\n'
            '  *) exit 2;;\n'
            'esac\n',
            encoding="utf-8", newline="\n",
        )
        for shim in shims.iterdir():
            shim.chmod(0o755)
        env = {key: value for key, value in os.environ.items() if not key.startswith("HYB_")}
        env.update({
            "HYB_INSTALL_DIR": shell_path(install),
            "HYB_BIN_DIR": shell_path(bin_dir),
            "HYB_TEST_INSTALLER": shell_path(ROOT / "install.sh"),
            "HYB_TEST_SHIMS": shell_path(shims),
            "HYB_TEST_ARCHIVE": shell_path(archive_path),
            "HYB_TEST_CHECKSUM": shell_path(checksum),
            "HYB_TEST_ASSET": asset,
            "HYB_TEST_TAG": tag,
            "HYB_TEST_REDIRECT_TAG": tag if via_redirect else "",
            "HYB_TEST_API_TAG": "" if via_redirect else tag,
            "HYB_TEST_EXECUTED": shell_path(executed),
        })
        self.latest = (env, install, executed)
        return self.latest

    def run_installer(self, scenario):
        env, _, _ = scenario
        return subprocess.run(
            [SHELL, "-c", 'PATH="$HYB_TEST_SHIMS:$PATH"; export PATH; exec sh "$HYB_TEST_INSTALLER"'],
            env=env, capture_output=True, text=True, encoding="utf-8", timeout=20,
        )

    def assert_full_tag_installed(self, tag, *, via_redirect=True):
        self.build_scenario(tag, via_redirect=via_redirect)
        env, install, executed = self.latest
        result = self.run_installer(self.latest)
        output = result.stdout + result.stderr
        self.assertEqual(result.returncode, 0, output)
        self.assertIn(f"==> release: {tag}", result.stdout)
        self.assertIn(f"==> downloading hybrid-{tag}-linux-amd64.tar.gz", result.stdout)
        self.assertIn("sha256 verified", result.stdout)
        self.assertTrue((install / "runtime/fixture.txt").exists(), output)
        self.assertTrue(executed.exists(), "the smoke test never ran")

    def test_redirect_tag_with_build_metadata_survives_resolution(self):
        self.assert_full_tag_installed("v1.2.3+build.7")

    def test_redirect_plain_tag_control(self):
        self.assert_full_tag_installed("v9.9.9")

    def test_redirect_prerelease_tag_control(self):
        self.assert_full_tag_installed("v1.2.3-rc.1")

    def test_api_fallback_tag_with_build_metadata_survives_resolution(self):
        self.assert_full_tag_installed("v2.0.0+meta.9", via_redirect=False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
