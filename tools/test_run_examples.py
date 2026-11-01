"""Offline CLI regressions for example exit-status and stdout checks.

The fake builder emits a small Python executable, so no Go/Rust toolchains
or network are used. These tests exercise the runner, not the compiler.
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
    def run_example(self, output=EXPECTED, exit_code=0, build_exit=0,
                    stderr="", example=EXAMPLE):
        with tempfile.TemporaryDirectory(prefix="hybrid-example-test-") as directory:
            root = Path(directory)
            program = (
                f"#!{sys.executable}\nimport sys\n"
                f"sys.stdout.write({output!r})\n"
                f"sys.stderr.write({stderr!r})\n"
                f"sys.exit({exit_code})\n"
            )
            builder = root / "hyb"
            builder.write_text(
                f"#!{sys.executable}\nfrom pathlib import Path\nimport sys\n"
                f"if {build_exit}:\n    sys.exit({build_exit})\n"
                "assert sys.argv[1] == 'build'\n"
                "if '--package' in sys.argv:\n    sys.exit(0)\n"
                "target = Path(sys.argv[sys.argv.index('-o') + 1])\n"
                f"target.write_text({program!r}, encoding='utf-8')\n"
                "target.chmod(0o755)\n",
                encoding="utf-8",
            )
            builder.chmod(0o755)
            # Example 09 runs the emitted package through Go. Use the same
            # executable fixture here instead of requiring a Go installation.
            go = root / "go"
            go.write_text(program, encoding="utf-8")
            go.chmod(0o755)
            # Keep the shell runner's scratch files inside the fixture too;
            # macOS's system mktemp may select its per-user temp root.
            mktemp = root / "mktemp"
            mktemp.write_text(
                f"#!{sys.executable}\nimport tempfile\n"
                f"print(tempfile.mkdtemp(dir={str(root)!r}))\n", encoding="utf-8")
            mktemp.chmod(0o755)
            selectors = [example] if isinstance(example, str) else list(example)
            return subprocess.run(
                ["bash", str(ROOT / "tools" / "run_examples.sh"), *selectors],
                env={**os.environ, "HYB_BIN": str(builder),
                     "TMPDIR": str(root),
                     "PATH": str(root) + os.pathsep + os.environ.get("PATH", "")},
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

    def check_streams(self, examples):
        for example in examples:
            expected = (ROOT / "examples" / example / "expected.txt").read_text(encoding="utf-8")
            with self.subTest(example=example, case="expected text on stderr"):
                result = self.run_example(output="", stderr=expected, example=example)
                self.assert_result(result, 1)
                self.assertIn(expected.splitlines()[0], result.stdout + result.stderr)
            with self.subTest(example=example, case="stdout with independent diagnostics"):
                result = self.run_example(output=expected, stderr="diagnostic only\n", example=example)
                self.assert_result(result, 0)
            with self.subTest(example=example, case="failed run preserves diagnostics"):
                result = self.run_example(output="wrong\n", stderr="runtime failed\n",
                                          exit_code=7, example=example)
                self.assert_result(result, 1)
                self.assertIn("runtime failed", result.stdout + result.stderr)

    def test_ordinary_examples_compare_only_stdout(self):
        self.check_streams([
            "01-hello", "03-worker-pool", "04-pipeline-backpressure",
            "05-json-transform", "06-http-server", "07-lru-generics", "08-graph-ownership",
        ])

    def test_package_example_compares_only_stdout(self):
        self.check_streams(["09-go-imports-hybrid"])

    def test_timing_example_compares_only_stdout(self):
        self.check_streams([EXAMPLE])


class ExampleSelectorTests(unittest.TestCase):
    run_example = ExampleExitTests.run_example
    assert_result = ExampleExitTests.assert_result

    EXAMPLES = (
        "01-hello", "02-cli-cat", "03-worker-pool", "04-pipeline-backpressure",
        "05-json-transform", "06-http-server", "07-lru-generics",
        "08-graph-ownership", "09-go-imports-hybrid", "10-rust-crate-import",
    )

    def test_documented_short_selectors_reach_the_builder(self):
        # A deliberate build failure pins selection for all ten examples,
        # including cat's separate runtime contract, without any toolchain.
        for example in self.EXAMPLES:
            with self.subTest(example=example):
                result = self.run_example(example=example[:2], build_exit=3)
                self.assert_result(result, 1)
                self.assertIn(f"--- {example}\n", result.stdout)
                self.assertIn(f"FAIL {example} (", result.stdout)
                self.assertNotIn("no program.hy", result.stdout)

    def test_short_selectors_match_full_names(self):
        for example in (STD_EXAMPLE, PKG_EXAMPLE, EXAMPLE):
            expected = (ROOT / "examples" / example / "expected.txt").read_text(encoding="utf-8")
            with self.subTest(example=example):
                short = self.run_example(example=example[:2], output=expected)
                full = self.run_example(example=example, output=expected)
                self.assert_result(short, 0)
                self.assert_result(full, 0)
                self.assertEqual(short.stdout, full.stdout)

    def test_multiple_short_and_full_selectors(self):
        result = self.run_example(example=["01", STD_EXAMPLE, "01"], output=STD_EXPECTED)
        self.assert_result(result, 0)
        self.assertEqual(result.stdout.count("PASS 01-hello (stdout == expected.txt)"), 3)

    def test_unknown_short_selector_still_fails(self):
        result = self.run_example(example="11")
        self.assert_result(result, 1)
        self.assertIn("FAIL 11 (no program.hy", result.stdout)

    def test_partial_and_missing_names_still_fail(self):
        for selector in ("0", "1", "01-hell", "missing-example"):
            with self.subTest(selector=selector):
                result = self.run_example(example=selector)
                self.assert_result(result, 1)
                self.assertIn(f"FAIL {selector} (no program.hy", result.stdout)

    def test_no_selectors_still_runs_all_examples(self):
        result = self.run_example(example=[], build_exit=3)
        self.assert_result(result, 1)
        selected = [line[4:] for line in result.stdout.splitlines() if line.startswith("--- ")]
        self.assertEqual(selected, list(self.EXAMPLES))


class StandardExampleExitTests(unittest.TestCase):
    # Reuse the shared fake-builder fixture without re-running the base
    # class's example-10 scenarios under this example's name.
    run_example = ExampleExitTests.run_example
    assert_result = ExampleExitTests.assert_result

    """PR #258: standard examples report a runtime exit as (run exit N) with
    the captured output, never as an output diff, and never via a missing
    .diff file error. Reuses the shared fake-builder fixture."""

    def run_std(self, output=STD_EXPECTED, exit_code=0, build_exit=0):
        return self.run_example(output=output, exit_code=exit_code,
                                build_exit=build_exit, example=STD_EXAMPLE)

    def test_matching_output_with_zero_exit_passes(self):
        self.assert_result(self.run_std(), 0)

    def test_matching_output_with_nonzero_exit_fails_as_run_exit(self):
        result = self.run_std(exit_code=7)
        self.assert_result(result, 1)
        self.assertIn("run exit 7", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_nonzero_exit_prints_runtime_output(self):
        result = self.run_std(output="fatal panic: test crash\n", exit_code=134)
        self.assert_result(result, 1)
        self.assertIn("run exit 134", result.stdout)
        self.assertIn("fatal panic: test crash", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_successful_run_with_wrong_output_fails_as_output_diff(self):
        result = self.run_std(output="wrong output\n")
        self.assert_result(result, 1)
        self.assertIn("output diff", result.stdout)

    def test_builder_failure_fails(self):
        result = self.run_std(build_exit=3)
        self.assert_result(result, 1)
        self.assertIn("FAIL 01-hello (build)", result.stdout)


class PackageModeExampleExitTests(unittest.TestCase):
    # Same shared fixture; the fake `go` in it drives the package-mode leg.
    run_example = ExampleExitTests.run_example
    assert_result = ExampleExitTests.assert_result

    """PR #258: package mode (09) applies the same run-exit contract to the
    `go run .` leg, through the shared fake-`go` fixture."""

    def run_pkg(self, output=PKG_EXPECTED, exit_code=0, build_exit=0):
        return self.run_example(output=output, exit_code=exit_code,
                                build_exit=build_exit, example=PKG_EXAMPLE)

    def test_matching_output_with_zero_exit_passes(self):
        self.assert_result(self.run_pkg(), 0)

    def test_matching_output_with_nonzero_exit_fails_as_run_exit(self):
        result = self.run_pkg(exit_code=9)
        self.assert_result(result, 1)
        self.assertIn("run exit 9", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_nonzero_exit_prints_runtime_output(self):
        result = self.run_pkg(output="runtime go panic: boom\n", exit_code=2)
        self.assert_result(result, 1)
        self.assertIn("run exit 2", result.stdout)
        self.assertIn("runtime go panic: boom", result.stdout)
        self.assertNotIn("No such file or directory", result.stderr)

    def test_successful_run_with_wrong_output_fails_as_output_diff(self):
        result = self.run_pkg(output="wrong output\n")
        self.assert_result(result, 1)
        self.assertIn("output diff", result.stdout)

    def test_builder_failure_fails(self):
        result = self.run_pkg(build_exit=4)
        self.assert_result(result, 1)
        self.assertIn("FAIL 09-go-imports-hybrid (package build)", result.stdout)


if __name__ == "__main__":
    unittest.main()
