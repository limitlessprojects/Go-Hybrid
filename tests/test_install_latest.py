#!/usr/bin/env python3
"""Run the real installer with offline latest-release headers and archives."""

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
RELEASES = "https://github.com/limitlessprojects/Go-Hybrid/releases"
API = "https://api.github.com/repos/limitlessprojects/Go-Hybrid/releases/latest"


class LatestReleaseTests(unittest.TestCase):
    def setUp(self):
        systems = {"Linux": "linux", "Darwin": "darwin"}
        arches = {"x86_64": "amd64", "aarch64": "arm64", "arm64": "arm64"}
        if platform.system() not in systems or platform.machine() not in arches:
            self.skipTest("installer supports Linux/macOS on amd64/arm64")
        self.platform = f"{systems[platform.system()]}-{arches[platform.machine()]}"
        tools = ("sh", "uname", "mktemp", "mkdir", "rm", "tar", "gzip", "cp",
                 "chmod", "cat", "sed", "head", "sha256sum", "shasum")
        paths = {name: shutil.which(name, path=os.defpath) for name in tools}
        if any(not paths[name] for name in tools[:-2]):
            self.skipTest("installer requires standard POSIX tools and gzip")
        if not paths["sha256sum"] and not paths["shasum"]:
            self.skipTest("installer requires a SHA-256 utility")
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-latest-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.install = self.root / "installed hybrid"
        self.bin_dir = self.root / "launcher bin"
        self.release = self.root / "release"
        self.release.mkdir()
        self.config = self.root / "responses.json"
        self.requests = self.root / "requests.jsonl"
        shim = self.root / "offline tools"
        shim.mkdir()
        for name, path in paths.items():
            if path:
                (shim / name).symlink_to(path)
        # Only local tools and this strict transport double are on PATH.
        (shim / "curl").write_text(
            f"#!{sys.executable} -IBS\n"
            "import json, os, pathlib, shutil, sys\n"
            "config = json.loads(pathlib.Path(os.environ['HYB_TEST_CONFIG']).read_text())\n"
            "args = sys.argv[1:]\n"
            "with open(os.environ['HYB_TEST_REQUESTS'], 'a') as log:\n"
            "    log.write(json.dumps(args) + '\\n')\n"
            f"if args == ['-fsSI', '{RELEASES}/latest']:\n"
            "    sys.stdout.write(config['headers'])\n"
            "elif len(args) == 4 and args[0] == '-fsSL' and args[2] == '-o':\n"
            "    url, output = args[1], args[3]\n"
            f"    if url == '{API}':\n"
            "        if config['api_tag'] is None:\n"
            "            sys.exit(22)\n"
            "        pathlib.Path(output).write_text(json.dumps({'tag_name': config['api_tag']}))\n"
            "    elif url == config['archive_url'] or url == config['archive_url'] + '.sha256':\n"
            "        shutil.copyfile(pathlib.Path(config['release']) / url.rsplit('/', 1)[1], output)\n"
            "    else:\n"
            "        sys.exit('unexpected release URL: ' + url)\n"
            "else:\n"
            "    sys.exit('unexpected offline request: ' + repr(args))\n",
            encoding="utf-8",
        )
        (shim / "curl").chmod(0o755)
        self.env = {
            "HOME": str(self.root), "PATH": str(shim), "LC_ALL": "C",
            "HYB_INSTALL_DIR": str(self.install), "HYB_BIN_DIR": str(self.bin_dir),
            "HYB_TEST_CONFIG": str(self.config), "HYB_TEST_REQUESTS": str(self.requests),
        }

    def run_installer(self, tag, headers, api_tag=None, pinned=False):
        payload = self.root / f"hybrid-{tag}"
        (payload / "runtime").mkdir(parents=True, exist_ok=True)
        (payload / "runtime" / "version").write_text(tag)
        (payload / "hyb").write_text(f"#!/bin/sh\nprintf '%s\\n' '{tag}'\n")
        (payload / "hyb").chmod(0o755)
        asset = f"hybrid-{tag}-{self.platform}.tar.gz"
        archive_path = self.release / asset
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(payload, arcname=payload.name)
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        (self.release / f"{asset}.sha256").write_text(f"{digest}  {asset}\n")
        self.archive_url = f"{RELEASES}/download/{tag}/{asset}"
        self.config.write_text(json.dumps({
            "headers": headers, "api_tag": api_tag,
            "archive_url": self.archive_url, "release": str(self.release),
        }))
        self.requests.write_text("")
        return subprocess.run(
            ["sh", str(ROOT / "install.sh")],
            env={**self.env, **({"HYB_VERSION": tag} if pinned else {})},
            cwd=self.root, capture_output=True, text=True, timeout=15,
        )

    def assert_installed(self, result, tag, api=False, pinned=False):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("==> sha256 verified", result.stdout)
        self.assertEqual((self.install / "runtime" / "version").read_text(), tag)
        probe = subprocess.run([str(self.bin_dir / "hyb")], env=self.env,
                               capture_output=True, text=True, timeout=5)
        self.assertEqual((probe.returncode, probe.stdout), (0, tag + "\n"))
        requests = [json.loads(line) for line in self.requests.read_text().splitlines()]
        expected_urls = [] if pinned else [f"{RELEASES}/latest"]
        if api:
            expected_urls.append(API)
        expected_urls.extend([self.archive_url, self.archive_url + ".sha256"])
        self.assertEqual([request[1] for request in requests], expected_urls)

    def test_ordinary_latest_release_installs_without_api(self):
        tag = "v1.2.3-rc.1"
        result = self.run_installer(tag, f"location: {RELEASES}/tag/{tag}\n")
        self.assert_installed(result, tag)

    def test_latest_release_preserves_build_metadata(self):
        tag = "v1.2.3+build.5"
        result = self.run_installer(tag, f"location: {RELEASES}/tag/{tag}\n")
        self.assert_installed(result, tag)

    def test_header_case_and_crlf_preserve_complete_tag(self):
        tag = "v1.2.3+build.5"
        result = self.run_installer(tag, f"HTTP/2 302\r\nLoCaTiOn: {RELEASES}/tag/{tag}\r\n\r\n")
        self.assert_installed(result, tag)

    def test_unparseable_redirect_uses_complete_api_tag(self):
        tag = "v1.2.3+build.5"
        for suffix in ("v1.2.3%2Bbuild.5", "v1.2.3/extra", "v1.2.3?extra",
                       "v9.0.0/tag/v1.2.3"):
            with self.subTest(suffix=suffix):
                result = self.run_installer(tag, f"location: {RELEASES}/tag/{suffix}\n", api_tag=tag)
                self.assert_installed(result, tag, api=True)

    def test_unparseable_redirect_and_failed_api_abort_before_download(self):
        for suffix in ("v1.2.3%2Bbuild.5", "v9.0.0/tag/v1.2.3"):
            with self.subTest(suffix=suffix):
                result = self.run_installer("v1.2.3", f"location: {RELEASES}/tag/{suffix}\n")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("could not resolve the latest release", result.stderr)
                requests = [json.loads(line) for line in self.requests.read_text().splitlines()]
                self.assertEqual(len(requests), 2)
                self.assertEqual(requests[1][:2], ["-fsSL", API])
                self.assertFalse(self.install.exists())

    def test_pinned_build_metadata_bypasses_latest_resolution(self):
        tag = "v1.2.3+build.5"
        result = self.run_installer(tag, "", pinned=True)
        self.assert_installed(result, tag, pinned=True)


if __name__ == "__main__":
    unittest.main(verbosity=2)
