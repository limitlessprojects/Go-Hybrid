#!/usr/bin/env python3
"""test_env_sh.py — regression tests for tools/env.sh CLI dispatch and check() pin validation (fixes #1127)."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ENV_SH = ROOT / "tools" / "env.sh"


def find_bash():
    for candidate in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
        r"C:\Program Files (x86)\Git\bin\bash.exe",
    ]:
        if os.path.exists(candidate):
            return candidate
    b = shutil.which("bash")
    if b and "System32" not in b and "system32" not in b:
        return b
    return "bash" if os.name != "nt" else None


BASH = find_bash()


@unittest.skipIf(BASH is None, "bash not found on system")
class EnvScriptTests(unittest.TestCase):
    def test_default_invocation_prints_summary(self):
        res = subprocess.run([BASH, str(ENV_SH)], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0)
        self.assertIn("Go:", res.stdout)
        self.assertIn("Rust:", res.stdout)
        self.assertIn("Cores:", res.stdout)

    def test_unknown_flag_exits_2(self):
        res = subprocess.run([BASH, str(ENV_SH), "--bogus"], capture_output=True, text=True)
        self.assertEqual(res.returncode, 2)
        self.assertIn("usage: tools/env.sh [--check]", res.stderr)

    def test_check_passes_when_pins_match_exact_token(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            bin_dir = Path(tmpdir)
            go_stub = bin_dir / "go"
            rust_stub = bin_dir / "rustc"
            go_stub.write_text("#!/bin/sh\necho 'go version go1.26.9 linux/amd64'\n")
            rust_stub.write_text("#!/bin/sh\necho 'rustc 1.99.0 (abcdef123 2026-10-21)'\n")
            os.chmod(go_stub, 0o755)
            os.chmod(rust_stub, 0o755)

            env = dict(os.environ)
            env["PATH"] = f"{bin_dir}{os.pathsep}{env.get('PATH', '')}"

            res = subprocess.run([BASH, str(ENV_SH), "--check"], env=env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, res.stdout + res.stderr)
            self.assertNotIn("WARN: go", res.stdout)
            self.assertNotIn("WARN: rustc", res.stdout)

    def test_check_fails_on_pin_mismatch(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            bin_dir = Path(tmpdir)
            go_stub = bin_dir / "go"
            rust_stub = bin_dir / "rustc"
            # Suffix/prerelease that previously false-passed regex substring match
            go_stub.write_text("#!/bin/sh\necho 'go version go1.26.90 linux/amd64'\n")
            rust_stub.write_text("#!/bin/sh\necho 'rustc 1.99.0-nightly (abcdef123 2026-10-21)'\n")
            os.chmod(go_stub, 0o755)
            os.chmod(rust_stub, 0o755)

            env = dict(os.environ)
            env["PATH"] = f"{bin_dir}{os.pathsep}{env.get('PATH', '')}"

            res = subprocess.run([BASH, str(ENV_SH), "--check"], env=env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 1)
            self.assertIn("WARN: go != go1.26.9", res.stdout)
            self.assertIn("WARN: rustc != 1.99.0", res.stdout)

    def test_check_fails_when_binary_is_missing(self):
        with tempfile.TemporaryDirectory() as empty_dir:
            env = dict(os.environ)
            env["PATH"] = str(empty_dir)

            res = subprocess.run([BASH, str(ENV_SH), "--check"], env=env, capture_output=True, text=True)
            self.assertEqual(res.returncode, 1)
            self.assertIn("WARN: go missing", res.stdout)
            self.assertIn("WARN: rustc missing", res.stdout)


if __name__ == "__main__":
    unittest.main()
