"""Controlled shell regressions for env.sh; not compiler integration tests.

Run the real script with fake version probes and real Bash/system utilities.
Only the machine-specific /home/z prefix is relocated in the temporary copy,
so installed compilers cannot mask a missing probe. No rg is put on PATH.
"""
import contextlib
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class EnvTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-env-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.home = self.root / "home"
        self.script = self.root / "env.sh"
        self.script.write_text(
            (ROOT / "tools/env.sh").read_text(encoding="utf-8")
            .replace("/home/z", str(self.home)), encoding="utf-8",
        )
        self.script.chmod(0o755)
        for tool in ("bash", "df", "free", "head", "tail", "awk", "nproc"):
            executable = shutil.which(tool)
            self.assertIsNotNone(executable, f"required shell utility: {tool}")
            (self.bin / tool).symlink_to(executable)
        self.log = self.root / "probes.log"
        self.probe("go", "go version go1.26.9 linux/amd64")
        self.probe("rustc", "rustc 1.99.0 (stable 2026-09-28)")
        self.probe("cargo", "cargo 1.99.0 (stable)")

    def probe(self, tool, output, status=0):
        path = self.bin / tool
        if output is None:
            path.unlink(missing_ok=True)
            return
        path.write_text(
            "#!/bin/sh\n"
            + f"printf '%s\\n' \"{tool} $*\" >> \"$PROBE_LOG\"\n"
            + "printf '%s\\n' " + shlex.quote(output) + "\n"
            + f"exit {status}\n", encoding="utf-8",
        )
        path.chmod(0o755)

    @contextlib.contextmanager
    def changed_probe(self, tool, output, status=0):
        path = self.bin / tool
        original = path.read_bytes()
        self.probe(tool, output, status)
        try:
            yield
        finally:
            path.write_bytes(original)
            path.chmod(0o755)

    def run_script(self, *args, source=None, strict=False):
        self.log.unlink(missing_ok=True)
        command = [str(self.script), *args]
        if source is not None:
            command = [str(self.bin / "bash"), "--noprofile", "--norc"]
            if strict:
                command += ["-e", "-u", "-o", "pipefail"]
            command += ["-c", source, "env-test", str(self.script), *args]
        return subprocess.run(
            command, cwd=self.root, capture_output=True, text=True,
            timeout=10, check=False,
            env={"PATH": str(self.bin), "HOME": str(self.root),
                 "LC_ALL": "C", "PROBE_LOG": str(self.log)},
        )

    def assertProbes(self, *expected):
        actual = self.log.read_text().splitlines() if self.log.exists() else []
        self.assertEqual(actual, list(expected))

    def assertCheck(self, status, warnings=(), source=False):
        result = (self.run_script(source='source "$1"; check', strict=True)
                  if source else self.run_script("--check"))
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertNotIn("Go:", result.stdout)
        self.assertNotIn("Cargo:", result.stdout)
        for tool in ("go", "rustc"):
            self.assertEqual(f"WARN: {tool}" in result.stdout, tool in warnings)
        return result

    def test_direct_check_dispatches_without_rg(self):
        self.assertFalse((self.bin / "rg").exists())
        self.assertCheck(0)
        self.assertProbes("go version", "rustc --version")

    def test_mismatched_versions_fail(self):
        for tool, output in (("go", "go version go1.26.8 linux/amd64"),
                             ("rustc", "rustc 1.98.0 (stable)")):
            with self.subTest(tool=tool), self.changed_probe(tool, output):
                self.assertCheck(1, (tool,))

    def test_suffixes_and_regex_collisions_fail(self):
        for tool, output in (
            ("go", "go version go1.26.90 linux/amd64"),
            ("go", "go version go1.26.9rc1 linux/amd64"),
            ("go", "go version go1X26X9 linux/amd64"),
            ("rustc", "rustc 1.99.0-nightly (hash date)"),
            ("rustc", "rustc 1.99.01 (hash date)"),
            ("rustc", "rustc 1X99X0 (hash date)"),
        ):
            with self.subTest(output=output), self.changed_probe(tool, output):
                self.assertCheck(1, (tool,))

    def test_expected_version_fields_are_required(self):
        for tool, output in (
            ("go", "go version go1.26.8 linux/amd64 go1.26.9"),
            ("go", "unexpected version go1.26.9 linux/amd64"),
            ("go", "go go1.26.9"),
            ("rustc", "rustc 1.98.0 (expected 1.99.0)"),
            ("rustc", "unexpected 1.99.0"),
            ("rustc", ""),
        ):
            with self.subTest(output=output), self.changed_probe(tool, output):
                self.assertCheck(1, (tool,))

    def test_whitespace_delimited_version_fields_pass(self):
        self.probe("go", "go\tversion\tgo1.26.9\tlinux/amd64")
        self.probe("rustc", "rustc\t1.99.0\t(hash date)")
        self.assertCheck(0)

    def test_missing_probes_fail_independently(self):
        for tool in ("go", "rustc"):
            with self.subTest(tool=tool), self.changed_probe(tool, None):
                self.assertCheck(1, (tool,))

    def test_failed_probe_cannot_pass_on_matching_output(self):
        for tool in ("go", "rustc"):
            output = ("go version go1.26.9 linux/amd64" if tool == "go"
                      else "rustc 1.99.0 (stable)")
            with self.subTest(tool=tool), self.changed_probe(tool, output, status=7):
                self.assertCheck(1, (tool,))
                self.assertProbes("go version", "rustc --version")

    def test_sourced_check_reports_both_failures_and_returns_status(self):
        self.probe("go", "go version go1.26.9 linux/amd64", status=7)
        self.probe("rustc", "rustc 1.99.0 (stable)", status=9)
        self.assertCheck(1, ("go", "rustc"), source=True)
        self.assertProbes("go version", "rustc --version")

    def test_sourced_check_passes_in_strict_shell(self):
        self.assertCheck(0, source=True)
        self.assertProbes("go version", "rustc --version")

    def test_source_only_exports_without_dispatch_or_exit(self):
        result = self.run_script("caller-arg", strict=True, source=(
            'source "$1" --check --extra; '
            'declare -F check >/dev/null; '
            'printf "%s\\n" "$PINNED_GO" "$PINNED_RUST" "$CARGO_BUILD_JOBS" '
            '"$GOFLAGS" "$MAKEFLAGS" "$PATH" "$2"'
        ))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertEqual(result.stdout.splitlines(), [
            "go1.26.9", "1.99.0", "2", "-p=2", "-j2",
            f"{self.home}/tools/go/bin:{self.home}/.cargo/bin:{self.bin}",
            "caller-arg",
        ])
        self.assertProbes()

    def test_no_arguments_keeps_summary(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stderr, "")
        self.assertTrue(result.stdout.startswith(
            "Go:      go version go1.26.9 linux/amd64\n"
            "Rust:    rustc 1.99.0 (stable 2026-09-28)\n"
            "Cargo:   cargo 1.99.0 (stable)\nKaggle:  MISSING\nCores:"
        ), result.stdout)
        self.assertNotIn("WARN:", result.stdout)
        self.assertProbes("go version", "rustc --version", "cargo --version")

    def test_invalid_arguments_fail_before_probing(self):
        for args in (("--unknown",), ("unexpected",), ("",),
                     ("--check", "--extra"), ("--check", "--check"),
                     ("", "--check")):
            with self.subTest(args=args):
                result = self.run_script(*args)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertIn("usage:", result.stderr.lower())
                self.assertProbes()


if __name__ == "__main__":
    unittest.main()
