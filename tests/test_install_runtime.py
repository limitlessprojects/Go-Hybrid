#!/usr/bin/env python3
"""Exercise install.sh's generated launcher against an offline release fixture.

The fixture executable only reports its environment and argv. These tests cover
installer/launcher behaviour, not compiler or Rust bridge correctness. No network
access, toolchain download, account, or third-party Python package is required.
"""

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
INSTALL_PATH_NAMES = (
    "ordinary",
    "with spaces",
    "$literal",
    "single'quote",
    'double"quote',
    r"back\slash",
    r"back\$literal",
    r"double\\$literal",
    "trailing\\",
    "literal`printf backtick`",
    "literal$(printf substitution)",
    "trailing-newline\n",
    "combined $literal 'single' \"double\" \\ end\\",
)
PROBE_ARGS = ["build", "file with spaces.hy", "", "$literal", "a'b", 'a"b', r"a\b", "end\\", "line\nbreak"]


class LauncherRuntimeTests(unittest.TestCase):
    def setUp(self):
        supported_os = {"Linux": "linux", "Darwin": "darwin"}
        supported_arch = {"x86_64": "amd64", "aarch64": "arm64", "arm64": "arm64"}
        if platform.system() not in supported_os or platform.machine() not in supported_arch:
            self.skipTest("install.sh supports Linux/macOS on amd64/arm64")
        required_tools = ("sh", "uname", "mktemp", "mkdir", "rm", "tar", "gzip", "cp", "chmod", "cat", "sed", "head")
        system_tools = {
            name: shutil.which(name, path=os.defpath)
            for name in (*required_tools, "sha256sum", "shasum")
        }
        if any(not system_tools[name] for name in required_tools):
            self.skipTest("the installer requires standard POSIX tools and gzip")
        if not system_tools["sha256sum"] and not system_tools["shasum"]:
            self.skipTest("the installer requires sha256sum or shasum")

        temporary = tempfile.TemporaryDirectory(prefix="hybrid-installer-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.install = self.root / "installed hybrid"
        self.bin_dir = self.root / "launcher bin"
        release = self.root / "release"
        release.mkdir()
        payload = self.root / "hybrid-v0.0.0-test"
        (payload / "runtime").mkdir(parents=True)
        (payload / "runtime" / "fixture.txt").write_text("offline test runtime\n", encoding="utf-8")
        (payload / "hyb").write_text(
            f"#!{sys.executable} -IBS\n"
            "import json, os, sys\n"
            "if sys.argv[1:] == ['explain', 'E0002']:\n"
            "    sys.exit(0)\n"
            "print(json.dumps({'runtime': os.environ.get('HYB_RUNTIME_DIR'), "
            "'executable': os.path.realpath(__file__), 'args': sys.argv[1:]}))\n",
            encoding="utf-8",
        )
        (payload / "hyb").chmod(0o755)
        asset = (
            "hybrid-v0.0.0-test-"
            f"{supported_os[platform.system()]}-{supported_arch[platform.machine()]}.tar.gz"
        )
        with tarfile.open(release / asset, "w:gz") as archive:
            archive.add(payload, arcname=payload.name)
        digest = hashlib.sha256((release / asset).read_bytes()).hexdigest()
        (release / f"{asset}.sha256").write_text(f"{digest}  {asset}\n", encoding="utf-8")

        shim_dir = self.root / "offline tools"
        shim_dir.mkdir()
        # Expose only the installer's local tools, never a real network client.
        for name, path in system_tools.items():
            if path:
                (shim_dir / name).symlink_to(path)
        (shim_dir / "curl").write_text(
            f"#!{sys.executable} -IBS\n"
            "import os, pathlib, shutil, sys\n"
            "args = sys.argv[1:]\n"
            "prefix = 'https://github.com/limitlessprojects/Go-Hybrid/releases/download/v0.0.0-test/'\n"
            "if pathlib.Path(sys.argv[0]).name == 'curl' and len(args) == 4 and args[0] == '-fsSL' and args[2] == '-o':\n"
            "    url, output = args[1], args[3]\n"
            "elif pathlib.Path(sys.argv[0]).name == 'wget' and len(args) == 3 and args[0] == '-qO':\n"
            "    url, output = args[2], args[1]\n"
            "else:\n"
            "    sys.exit('unexpected download request in offline test')\n"
            "if not url.startswith(prefix):\n"
            "    sys.exit('unexpected download request in offline test')\n"
            "name = url[len(prefix):]\n"
            "if name != pathlib.Path(name).name:\n"
            "    sys.exit('unexpected asset path in offline test')\n"
            "shutil.copyfile(pathlib.Path(os.environ['HYB_TEST_RELEASE_DIR']) / name, output)\n",
            encoding="utf-8",
        )
        (shim_dir / "curl").chmod(0o755)
        (shim_dir / "wget").symlink_to("curl")
        (self.root / "home").mkdir()
        (self.root / "tmp").mkdir()
        # A fresh environment excludes credentials, shell hooks, and user paths.
        self.env = {
            "HOME": str(self.root / "home"),
            "TMPDIR": str(self.root / "tmp"),
            "PATH": str(shim_dir),
            "LC_ALL": "C",
            "HYB_VERSION": "v0.0.0-test",
            "HYB_INSTALL_DIR": str(self.install),
            "HYB_BIN_DIR": str(self.bin_dir),
            "HYB_TEST_RELEASE_DIR": str(release),
        }

    def install_fixture(self, **overrides):
        result = subprocess.run(
            ["sh", str(ROOT / "install.sh")],
            env={**self.env, **overrides},
            cwd=self.root,
            capture_output=True, text=True, timeout=15, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("==> sha256 verified", result.stdout)

    def probe(self, *args, cwd=None, **overrides):
        env = dict(self.env)
        # Exercise the baked-in installation location, as a fresh shell does.
        env.pop("HYB_INSTALL_DIR", None)
        env.update(overrides)
        result = subprocess.run(
            [str(self.bin_dir / "hyb"), *args], env=env,
            cwd=self.root if cwd is None else cwd,
            capture_output=True, text=True, timeout=15, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def test_unset_runtime_uses_packaged_default(self):
        self.install_fixture()
        self.assertEqual(self.probe()["runtime"], str(self.install / "runtime"))

    def test_empty_runtime_uses_packaged_default(self):
        self.install_fixture()
        self.assertEqual(self.probe(HYB_RUNTIME_DIR="")["runtime"], str(self.install / "runtime"))

    def test_relative_install_paths_work_from_another_directory(self):
        for name in ("relative install", "$literal 'quoted'", "trailing-newline\n"):
            with self.subTest(install=name):
                self.install_fixture(HYB_INSTALL_DIR=name)
                result = self.probe(*PROBE_ARGS, cwd=self.root / "home")
                installed = (self.root / name).resolve()
                self.assertTrue(Path(result["runtime"]).is_absolute())
                self.assertEqual(Path(result["runtime"]).resolve(), installed / "runtime")
                self.assertEqual(result["executable"], str(installed / "bin" / "hyb"))
                self.assertEqual(result["args"], PROBE_ARGS)

    def test_relative_install_preserves_runtime_and_install_overrides(self):
        self.install_fixture(HYB_INSTALL_DIR="relative install")
        relocated = self.root / "relocated hybrid"
        shutil.copytree(self.root / "relative install", relocated)
        runtime = str(self.root / "custom runtime $literal 'quoted'")
        result = self.probe(
            *PROBE_ARGS, cwd=self.root / "home",
            HYB_INSTALL_DIR=str(relocated), HYB_RUNTIME_DIR=runtime,
        )
        self.assertEqual(result["runtime"], runtime)
        self.assertEqual(result["executable"], str((relocated / "bin" / "hyb").resolve()))
        self.assertEqual(result["args"], PROBE_ARGS)

    def test_explicit_runtime_is_preserved_verbatim(self):
        self.install_fixture()
        runtime = str(self.root / "custom runtime $literal 'quoted'")
        Path(runtime).mkdir()
        self.assertEqual(self.probe(HYB_RUNTIME_DIR=runtime)["runtime"], runtime)

    def test_runtime_can_change_between_invocations(self):
        self.install_fixture()
        for name in ("runtime one", "runtime two"):
            runtime = str(self.root / name)
            Path(runtime).mkdir()
            with self.subTest(runtime=name):
                self.assertEqual(self.probe(HYB_RUNTIME_DIR=runtime)["runtime"], runtime)

    def test_install_dir_override_sets_default_runtime(self):
        self.install_fixture()
        relocated = self.root / "relocated hybrid"
        shutil.copytree(self.install, relocated)
        self.assertEqual(
            self.probe(HYB_INSTALL_DIR=str(relocated))["runtime"],
            str(relocated / "runtime"),
        )

    def test_explicit_runtime_wins_over_install_dir_override(self):
        self.install_fixture()
        relocated = self.root / "relocated hybrid"
        shutil.copytree(self.install, relocated)
        runtime = str(self.root / "separate runtime")
        Path(runtime).mkdir()
        self.assertEqual(
            self.probe(HYB_INSTALL_DIR=str(relocated), HYB_RUNTIME_DIR=runtime)["runtime"],
            runtime,
        )

    def test_install_time_runtime_is_not_baked_into_launcher(self):
        self.install_fixture(HYB_RUNTIME_DIR=str(self.root / "install-time runtime"))
        self.assertEqual(self.probe()["runtime"], str(self.install / "runtime"))

    def test_arguments_are_forwarded_unchanged(self):
        self.install_fixture()
        args = ["build", "file with spaces.hy", "", "$literal", "a'b"]
        self.assertEqual(self.probe(*args)["args"], args)

    def test_install_paths_preserve_runtime_executable_and_arguments(self):
        for name in INSTALL_PATH_NAMES:
            with self.subTest(install=name):
                installed = self.root / name
                self.install_fixture(HYB_INSTALL_DIR=str(installed))
                expected = {
                    "runtime": str(installed / "runtime"),
                    "executable": str((installed / "bin" / "hyb").resolve()),
                    "args": PROBE_ARGS,
                }
                self.assertEqual(self.probe(*PROBE_ARGS), expected)
                runtime = str(self.root / "runtime overrides" / name)
                self.assertEqual(
                    self.probe(*PROBE_ARGS, HYB_RUNTIME_DIR=runtime),
                    {**expected, "runtime": runtime},
                )

    def test_special_install_overrides_preserve_runtime_precedence(self):
        self.install_fixture()
        for name in INSTALL_PATH_NAMES:
            with self.subTest(install_override=name):
                relocated = self.root / "relocated" / name
                shutil.copytree(self.install, relocated)
                expected = {
                    "runtime": str(relocated / "runtime"),
                    "executable": str((relocated / "bin" / "hyb").resolve()),
                    "args": PROBE_ARGS,
                }
                self.assertEqual(
                    self.probe(*PROBE_ARGS, HYB_INSTALL_DIR=str(relocated)), expected,
                )
                runtime = str(self.root / "runtime overrides" / name)
                self.assertEqual(
                    self.probe(*PROBE_ARGS, HYB_INSTALL_DIR=str(relocated), HYB_RUNTIME_DIR=runtime),
                    {**expected, "runtime": runtime},
                )


    def test_unsafe_hyb_version_is_rejected_before_network(self):
        for bad in ("../evil", "-rf", "..", "v1/2"):
            r = subprocess.run(["sh", str(ROOT / "install.sh")], env={**os.environ, "HYB_VERSION": bad}, capture_output=True, text=True, timeout=5)
            self.assertEqual(r.returncode, 2)

    def test_relative_bin_dir_resolves_against_pwd(self):
        r = subprocess.run(["sh", str(ROOT / "install.sh")], env={**self.env, "HYB_BIN_DIR": "rel-bin"}, cwd=self.root, capture_output=True, text=True, timeout=15)
        self.assertIn(f'export PATH="{self.root}/rel-bin:$PATH"', r.stdout)

if __name__ == "__main__":
    unittest.main(verbosity=2)
