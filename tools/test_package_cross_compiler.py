"""Offline regressions for cross-compiler argument boundaries in packaging.

Run the real POSIX shell script and archive/checksum tools; replace only the
compiler/toolchain boundary. The logs record arguments received by Cargo and
Go, including both release builds and both linux/arm64 smoke builds.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
LINKER_KEY = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"


class CrossCompilerTests(unittest.TestCase):
    def package(self, linker=None, expected_error=None):
        with tempfile.TemporaryDirectory(prefix="hybrid-cross-test-") as directory:
            root = Path(directory)
            repo = root / "repo"
            for name in ("tools", "compiler", "runtime/rust/hyrt/src", "runtime/go",
                         "examples/01-hello", "examples/05-json-transform"):
                (repo / name).mkdir(parents=True)
            shutil.copy2(ROOT / "tools/package_release.sh", repo / "tools/package_release.sh")
            (repo / "tools/RELEASE-INSTALL.md").write_text("fixture\n")
            (repo / "runtime/rust/hyrt/src/lib.rs").write_text("// fixture\n")
            for example in ("01-hello", "05-json-transform"):
                (repo / "examples" / example / "program.hy").write_text("fn main() {}\n")
            cargo_bin = root / "cargo-bin"
            go_bin = root / "go-bin"
            cargo_bin.mkdir()
            go_bin.mkdir()
            log = root / "commands.jsonl"
            # Deliberate matching filenames make the unquoted linker assignment
            # expand as a glob, instead of relying on no-match shell behavior.
            for cwd in (repo / "compiler", repo / "runtime/rust/hyrt"):
                (cwd / f"{LINKER_KEY}=cc1").touch()
            cargo = cargo_bin / "cargo"
            cargo.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "args = sys.argv[1:]\n"
                f"record = {{'tool': 'cargo', 'linker': os.environ.get({LINKER_KEY!r}), 'args': args}}\n"
                "with open(os.environ['COMMAND_LOG'], 'a') as log:\n"
                "    log.write(json.dumps(record) + '\\n')\n"
                "target = pathlib.Path('target')\n"
                "if '--target' in args:\n"
                "    target /= args[args.index('--target') + 1]\n"
                "target /= 'release'\n"
                "target.mkdir(parents=True, exist_ok=True)\n"
                "if '-p' in args:\n"
                "    binary = target / 'hyb'\n"
                "    binary.write_text('#!/bin/sh\\nexec go \"$@\"\\n')\n"
                "    binary.chmod(0o755)\n"
                "else:\n"
                "    (target / 'libhyrt.a').write_text('fixture archive\\n')\n"
            )
            go = go_bin / "go"
            go.write_text(
                f"#!{sys.executable}\n"
                "import json, os, pathlib, sys\n"
                "args = sys.argv[1:]\n"
                "record = {'tool': 'go', 'cc': os.environ.get('CC'),\n"
                "          'arch': os.environ.get('GOARCH'), 'cgo': os.environ.get('CGO_ENABLED'), 'args': args}\n"
                "with open(os.environ['COMMAND_LOG'], 'a') as log:\n"
                "    log.write(json.dumps(record) + '\\n')\n"
                "output = 'hello, Hybrid!' if '01-hello' in args[1] else '\"ada\"'\n"
                "binary = pathlib.Path(args[args.index('-o') + 1])\n"
                "binary.write_text(\"#!/bin/sh\\nprintf '%s\\\\n' '\" + output + \"'\\n\")\n"
                "binary.chmod(0o755)\n"
            )
            qemu = go_bin / "qemu-fixture"
            qemu.write_text('#!/bin/sh\n[ "$1" = --fixture ] || exit 91\nshift\nexec "$@"\n')
            for command in (cargo, go, qemu):
                command.chmod(0o755)
            env = {
                "PATH": os.pathsep.join((str(cargo_bin), str(go_bin), os.defpath)),
                "HOME": str(root), "TMPDIR": str(root), "COMMAND_LOG": str(log),
                "HYB_PKG_OS": "linux", "HYB_PKG_ARCH": "amd64",
                "CC": "native-cc", "GOARCH": "amd64", "CGO_ENABLED": "0",
            }
            if linker is not None:
                env.update(HYB_RUST_TARGET="aarch64-unknown-linux-gnu", HYB_CC=linker,
                           HYB_PKG_ARCH="arm64", HYB_QEMU="qemu-fixture --fixture")
            result = subprocess.run(
                ["sh", str(repo / "tools/package_release.sh"), "v1.2.3"],
                cwd=root, env=env, capture_output=True, text=True, timeout=15,
            )
            if expected_error is not None:
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn(expected_error, result.stderr)
                return []
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            return [json.loads(line) for line in log.read_text().splitlines()]

    def assert_cross_compiler(self, linker, go_cc):
        records = self.package(linker)
        cargo = [record for record in records if record["tool"] == "cargo"]
        go = [record for record in records if record["tool"] == "go"]
        self.assertEqual(len(cargo), 2)
        self.assertEqual([record["linker"] for record in cargo], [linker, linker])
        self.assertEqual(len(go), 2)
        self.assertEqual([record["cc"] for record in go], [go_cc, go_cc])
        self.assertEqual([record["arch"] for record in go], ["arm64", "arm64"])
        self.assertEqual([record["cgo"] for record in go], ["1", "1"])
        self.assertIn("01-hello", go[0]["args"][1])
        self.assertIn("05-json-transform", go[1]["args"][1])

    def test_cross_compiler_path_with_spaces(self):
        self.assert_cross_compiler("/opt/tool chain/bin/cc", "'/opt/tool chain/bin/cc'")

    def test_cross_compiler_glob_characters_stay_literal(self):
        self.assert_cross_compiler("cc[1]", "cc[1]")

    def test_cross_compiler_paths_with_quotes(self):
        self.assert_cross_compiler("/opt/it's a compiler/cc", '"/opt/it\'s a compiler/cc"')
        self.assert_cross_compiler('/opt/"compiler suite"/cc', '\'/opt/"compiler suite"/cc\'')

    def test_plain_cross_compiler(self):
        self.assert_cross_compiler("aarch64-linux-gnu-gcc", "aarch64-linux-gnu-gcc")

    def test_go_compiler_path_with_both_quote_kinds_fails_clearly(self):
        self.package('/opt/it\'s a "compiler"/cc', expected_error="Go CC cannot quote")

    def test_native_build_preserves_caller_go_environment(self):
        records = self.package()
        cargo = [record for record in records if record["tool"] == "cargo"]
        go = [record for record in records if record["tool"] == "go"]
        self.assertEqual(len(cargo), 2)
        self.assertTrue(all(record["linker"] is None for record in cargo))
        self.assertEqual(len(go), 1)
        self.assertEqual((go[0]["cc"], go[0]["arch"], go[0]["cgo"]),
                         ("native-cc", "amd64", "0"))


if __name__ == "__main__":
    unittest.main()
