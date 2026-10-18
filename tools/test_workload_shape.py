"""Offline CLI regressions for the workload generator's read-only check mode."""
from pathlib import Path
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
GENERATED = (
    "go/main.go",
    "rust/src/main.rs",
    "hy/load.hy",
    "hy/alloc.hy",
    "rust/Cargo.toml",
)


class WorkloadShapeTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-shape-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.workloads = self.root / "workloads"
        for name in ("shape.py",) + GENERATED:
            target = self.workloads / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "benchmarks/workloads" / name, target)
            # Git may have checked out CRLF on Windows. The generator writes
            # canonical LF, so make byte-for-byte regeneration tests portable.
            target.write_bytes(target.read_bytes().replace(b"\r\n", b"\n"))
            # A fixed old mtime detects even a byte-identical rewrite.
            os.utime(target, (1_000_000_000, 1_000_000_000))

    def run_shape(self, *args, env=None):
        run_env = os.environ.copy()
        if env:
            run_env.update(env)
        return subprocess.run(
            [sys.executable, str(self.workloads / "shape.py"), *args],
            cwd=self.root, capture_output=True, text=True, timeout=10,
            env=run_env,
        )

    def snapshot(self):
        return {
            name: (hashlib.sha256(path.read_bytes()).hexdigest(),
                   path.stat().st_mtime_ns, path.stat().st_mode)
            for name in GENERATED
            if (path := self.workloads / name).exists()
        }

    def assert_check_preserves_tree(self, expected_code):
        before = self.snapshot()
        result = self.run_shape("--check")
        self.assertEqual(self.snapshot(), before, result.stdout + result.stderr)
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        return result

    def test_matching_outputs_are_not_rewritten(self):
        result = self.assert_check_preserves_tree(0)
        self.assertIn("triplet shapes in sync", result.stdout)

    def test_hybrid_alloc_caches_alphabet_outside_timed_work(self):
        """The Rust twin initializes its alphabet once; Hybrid must not
        rebuild a slice twice per byte inside the measured allocation loop.
        """
        source = (self.workloads / "hy/alloc.hy").read_text(encoding="utf-8")
        main = source.split("fn main() {", 1)[1]
        self.assertEqual(main.count("alphabet()"), 1)
        self.assertIn("alphabet_values := alphabet()", main)
        self.assertLess(main.index("alphabet_values := alphabet()"),
                        main.index("for w in 0..windows"))
        self.assertIn(
            "append(item, alphabet_values[(j+k)%len(alphabet_values)])", main)

    def test_manifest_drift_fails_without_repairing_it(self):
        manifest = self.workloads / "rust/Cargo.toml"
        manifest.write_text(manifest.read_text(encoding="utf-8").replace("opt-level = 3", "opt-level = 0"), encoding="utf-8")
        result = self.assert_check_preserves_tree(1)
        self.assertIn("rust/Cargo.toml", result.stdout)

    def test_missing_manifest_fails_without_creating_it(self):
        (self.workloads / "rust/Cargo.toml").unlink()
        result = self.assert_check_preserves_tree(1)
        self.assertIn("rust/Cargo.toml", result.stdout)

    def test_program_drift_fails_without_rewriting_other_outputs(self):
        (self.workloads / "go/main.go").write_text("// drifted\n", encoding="utf-8")
        result = self.assert_check_preserves_tree(1)
        self.assertIn("go/main.go", result.stdout)

    def test_missing_program_reports_drift_without_traceback(self):
        (self.workloads / "hy/load.hy").unlink()
        result = self.assert_check_preserves_tree(1)
        self.assertIn("hy/load.hy", result.stdout)

    def assert_args_preserve_tree(self, args, expected_code):
        # A drifted file distinguishes validation from accidental regeneration.
        (self.workloads / "go/main.go").write_text("// keep this drifted file\n")
        before = self.snapshot()
        result = self.run_shape(*args)
        self.assertEqual(self.snapshot(), before, result.stdout + result.stderr)
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertNotIn("wrote ", result.stdout)
        self.assertNotIn("Traceback", result.stderr)
        return result

    def test_unknown_flags_fail_without_writing_outputs(self):
        for args in (("--chek",), ("--check", "--unexpected")):
            with self.subTest(args=args):
                result = self.assert_args_preserve_tree(args, 2)
                self.assertIn("unrecognized arguments", result.stderr)

    def test_positional_arguments_fail_without_writing_outputs(self):
        for args in (("extra",), ("--check", "extra")):
            with self.subTest(args=args):
                result = self.assert_args_preserve_tree(args, 2)
                self.assertIn("unrecognized arguments", result.stderr)

    def test_help_does_not_write_outputs(self):
        result = self.assert_args_preserve_tree(("--help",), 0)
        self.assertIn("usage:", result.stdout)
        self.assertIn("--check", result.stdout)

    def test_generation_restores_all_five_outputs(self):
        expected = {name: (self.workloads / name).read_bytes() for name in GENERATED}
        for name in GENERATED:
            (self.workloads / name).unlink()
        result = self.run_shape()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            {name: (self.workloads / name).read_bytes() for name in GENERATED}, expected,
        )

    def test_check_succeeds_under_non_utf8_locale(self):
        result = self.run_shape("--check", env={"LC_ALL": "C", "PYTHONUTF8": "0"})
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("triplet shapes in sync", result.stdout)
        self.assertNotIn("Traceback", result.stderr)

    def test_generation_succeeds_under_non_utf8_locale(self):
        for name in GENERATED:
            (self.workloads / name).unlink()
        result = self.run_shape(env={"LC_ALL": "C", "PYTHONUTF8": "0"})
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        for name in GENERATED:
            self.assertTrue((self.workloads / name).exists())


if __name__ == "__main__":
    unittest.main()
