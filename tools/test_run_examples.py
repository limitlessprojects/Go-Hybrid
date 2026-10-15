"""Offline CLI regressions for example runners' exit-status and output checks.

The fake builder and fake runner emit small Python executables, so no Go/Rust
toolchains or network are used. These tests exercise the runner, not the compiler.
"""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
EXAMPLE = "10-rust-crate-import"
EXPECTED = (ROOT / "examples" / EXAMPLE / "expected.txt").read_text(encoding="utf-8")

STD_EXAMPLE = "01-hello"
STD_EXPECTED = (ROOT / "examples" / STD_EXAMPLE / "expected.txt").read_text(encoding="utf-8")

PKG_EXAMPLE = "09-go-imports-hybrid"
PKG_EXPECTED = (ROOT / "examples" / PKG_EXAMPLE / "expected.txt").read_text(encoding="utf-8")


class ExampleExitTests(unittest.TestCase):
    def run_example(self, output=EXPECTED, exit_code=0, build_exit=0):
        with tempfile.TemporaryDirectory(prefix="hybrid-example-test-") as directory:
            root = Path(directory)
            program = (
                f"#!{sys.executable}\nimport sys\n"
                f"sys.stdout.write({output!r})\n"
                f"sys.exit({exit_code})\n"
            )
            builder = root / "hyb"
            builder.write_text(
                f"#!{sys.executable}\nfrom pathlib import Path\nimport sys\n"
                f"if {build_exit}:\n    sys.exit({build_exit})\n"
                "assert sys.argv[1] == 'build'\n"
                "target = Path(sys.argv[sys.argv.index('-o') + 1])\n"
                f"target.write_text({program!r}, encoding='utf-8')\n"
                "target.chmod(0o755)\n",
                encoding="utf-8",
            )
            builder.chmod(0o755)
            return subprocess.run(
                ["bash", str(ROOT / "tools" / "run_examples.sh"), EXAMPLE],
                env={**os.environ, "HYB_BIN": str(builder)},
                capture_output=True, text=True, encoding="utf-8", timeout=15,
            )

    def assert_result(self, result, expected_code):
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertEqual("examples: ALL PASS" in result.stdout, expected_code == 0)

    def test_matching_output_with_zero_exit_passes(self):
        self.assert_result(self.run_example(), 0)

    def test_successful_timing_is_still_normalized(self):
        output = EXPECTED.replace("in 0ms,", "in 123ms,")
        self.assert_result(self.run_example(output=output), 0)

    def test_matching_output_with_nonzero_exit_fails(self):
        result = self.run_example(exit_code=7)
        self.assert_result(result, 1)
        self.assertIn("run exit 7", result.stdout)

    def test_normalized_matching_output_with_nonzero_exit_fails(self):
        output = EXPECTED.replace("in 0ms,", "in 123ms,")
        result = self.run_example(output=output, exit_code=1)
        self.assert_result(result, 1)
        self.assertIn("run exit 1", result.stdout)

    def test_successful_run_with_wrong_output_fails(self):
        self.assert_result(self.run_example(output="wrong result\n"), 1)

    def test_builder_failure_still_fails(self):
        self.assert_result(self.run_example(build_exit=3), 1)


class StandardExampleExitTests(unittest.TestCase):
    def run_example(self, output=STD_EXPECTED, exit_code=0, build_exit=0):
        with tempfile.TemporaryDirectory(prefix="hybrid-std-example-test-") as directory:
            root = Path(directory)
            program = (
                f"#!{sys.executable}\nimport sys\n"
                f"sys.stdout.write({output!r})\n"
                f"sys.exit({exit_code})\n"
            )
            builder = root / "hyb"
            builder.write_text(
                f"#!{sys.executable}\nfrom pathlib import Path\nimport sys\n"
                f"if {build_exit}:\n    sys.exit({build_exit})\n"
                "assert sys.argv[1] == 'build'\n"
                "target = Path(sys.argv[sys.argv.index('-o') + 1])\n"
                f"target.write_text({program!r}, encoding='utf-8')\n"
                "target.chmod(0o755)\n",
                encoding="utf-8",
            )
            builder.chmod(0o755)
            return subprocess.run(
                ["bash", str(ROOT / "tools" / "run_examples.sh"), STD_EXAMPLE],
                env={**os.environ, "HYB_BIN": str(builder)},
                capture_output=True, text=True, encoding="utf-8", timeout=15,
            )

    def assert_result(self, result, expected_code):
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertEqual("examples: ALL PASS" in result.stdout, expected_code == 0)

    def test_standard_matching_output_with_zero_exit_passes(self):
        self.assert_result(self.run_example(), 0)

    def test_standard_matching_output_with_nonzero_exit_fails(self):
        result = self.run_example(exit_code=7)
        self.assert_result(result, 1)
        self.assertIn("run exit 7", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_standard_nonzero_exit_prints_runtime_output(self):
        result = self.run_example(output="fatal panic: test crash\n", exit_code=134)
        self.assert_result(result, 1)
        self.assertIn("run exit 134", result.stdout)
        self.assertIn("fatal panic: test crash", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_standard_successful_run_with_wrong_output_fails(self):
        result = self.run_example(output="wrong output\n")
        self.assert_result(result, 1)
        self.assertIn("output diff", result.stdout)

    def test_standard_builder_failure_fails(self):
        result = self.run_example(build_exit=3)
        self.assert_result(result, 1)
        self.assertIn("FAIL 01-hello (build)", result.stdout)


class PackageModeExampleExitTests(unittest.TestCase):
    def run_example(self, output=PKG_EXPECTED, exit_code=0, build_exit=0):
        with tempfile.TemporaryDirectory(prefix="hybrid-pkg-example-test-") as directory:
            root = Path(directory)
            builder = root / "hyb"
            builder.write_text(
                f"#!{sys.executable}\nimport sys\n"
                f"if {build_exit}:\n    sys.exit({build_exit})\n"
                "sys.exit(0)\n",
                encoding="utf-8",
            )
            builder.chmod(0o755)

            fake_go = root / "go"
            fake_go.write_text(
                f"#!{sys.executable}\nimport sys\n"
                "if sys.argv[1:3] == ['run', '.']:\n"
                f"    sys.stdout.write({output!r})\n"
                f"    sys.exit({exit_code})\n"
                "sys.exit(0)\n",
                encoding="utf-8",
            )
            fake_go.chmod(0o755)

            env = {
                **os.environ,
                "HYB_BIN": str(builder),
                "PATH": f"{root}:{os.environ.get('PATH', '')}",
            }
            return subprocess.run(
                ["bash", str(ROOT / "tools" / "run_examples.sh"), PKG_EXAMPLE],
                env=env,
                capture_output=True, text=True, encoding="utf-8", timeout=15,
            )

    def assert_result(self, result, expected_code):
        self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
        self.assertEqual("examples: ALL PASS" in result.stdout, expected_code == 0)

    def test_package_mode_matching_output_with_zero_exit_passes(self):
        self.assert_result(self.run_example(), 0)

    def test_package_mode_matching_output_with_nonzero_exit_fails(self):
        result = self.run_example(exit_code=9)
        self.assert_result(result, 1)
        self.assertIn("run exit 9", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_package_mode_nonzero_exit_prints_runtime_output(self):
        result = self.run_example(output="runtime go panic: boom\n", exit_code=2)
        self.assert_result(result, 1)
        self.assertIn("run exit 2", result.stdout)
        self.assertIn("runtime go panic: boom", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_package_mode_successful_run_with_wrong_output_fails(self):
        result = self.run_example(output="wrong output\n")
        self.assert_result(result, 1)
        self.assertIn("output diff", result.stdout)

    def test_package_mode_builder_failure_fails(self):
        result = self.run_example(build_exit=4)
        self.assert_result(result, 1)
        self.assertIn("FAIL 09-go-imports-hybrid (package build)", result.stdout)


if __name__ == "__main__":
    unittest.main()
