"""Offline failure-propagation and scratch-isolation checks for the workload runner."""
from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]


class WorkloadRunTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-workload-run-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.workloads = self.root / "benchmarks/workloads"
        for directory in ("go", "rust", "hy"):
            (self.workloads / directory).mkdir(parents=True)
        (self.root / "runtime/go").mkdir(parents=True)
        (self.root / "runtime/go/runtime.go").write_text("package main\n")
        (self.workloads / "go/main.go").write_text("package main\n")

        # Safely reproduce old versions' fixed /tmp outputs without changing
        # host-wide files. Per-run directories use the isolated TMPDIR below.
        script = (ROOT / "benchmarks/workloads/run.sh").read_text()
        for name in ("gowl", "wl-go", "wl-hy-load", "wl-hy-alloc"):
            script = script.replace("/tmp/" + name, str(self.root / name))
        self.runner = self.workloads / "run.sh"
        self.runner.write_text(script)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.scratch = self.root / "temporary files"
        self.scratch.mkdir()
        stub = "#!" + sys.executable + "\n" + '''
import os
from pathlib import Path
import sys
import time

tool = Path(sys.argv[0]).name
args = sys.argv[1:]
stage = tool
if tool == "hyb":
    stage += " " + args[0] + " " + Path(args[1]).stem
with open(os.environ["TRACE"], "a") as trace:
    trace.write(stage + "\\n")
if stage == os.environ.get("FAIL_STAGE"):
    sys.exit(17)
if tool == "go":
    if os.environ.get("GO_READY"):
        marker = Path("first-builder-input")
        marker.write_text("keep this input")
        Path(os.environ["GO_READY"]).touch()
        deadline = time.monotonic() + 15
        while not Path(os.environ["GO_RELEASE"]).exists():
            if time.monotonic() > deadline:
                sys.exit(24)
            time.sleep(0.01)
        if not marker.exists():
            sys.exit(23)
    output = Path(args[args.index("-o") + 1])
elif tool == "cargo":
    output = Path("target/release/workloads")
elif tool == "hyb" and args[0] == "build":
    output = Path(args[args.index("-o") + 1])
else:
    sys.exit(0)
output.parent.mkdir(parents=True, exist_ok=True)
output.write_text("#!/bin/sh\\nprintf '%s\\\\n' 'run " + output.name + "' >> \\\"$TRACE\\\"\\n")
output.chmod(0o755)
'''
        for tool in ("go", "cargo", "hyb"):
            path = self.bin / tool
            path.write_text(stub)
            path.chmod(0o755)
        self.trace = self.root / "trace"

    def triplet_env(self, failure=""):
        return {
            "PATH": str(self.bin) + os.pathsep + os.defpath,
            "HOME": str(self.root / "home"),
            "HYB": str(self.bin / "hyb"),
            "TRACE": str(self.trace),
            "FAIL_STAGE": failure,
            "TMPDIR": str(self.scratch),
        }

    def run_triplet(self, failure=""):
        result = subprocess.run(
            ["bash", str(self.runner)], env=self.triplet_env(failure), cwd=self.root,
            capture_output=True, text=True, timeout=10,
        )
        trace = self.trace.read_text().splitlines()
        return result, trace

    def test_success_runs_both_hybrid_workloads(self):
        result, trace = self.run_triplet()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("run wl-hy-load", trace)
        self.assertIn("run wl-hy-alloc", trace)

    def test_load_build_failure_stops_before_alloc_build(self):
        result, trace = self.run_triplet("hyb build load")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertNotIn("run wl-hy-load", trace)
        self.assertNotIn("hyb build alloc", trace)

    def test_alloc_build_failure_is_not_reported_as_success(self):
        result, trace = self.run_triplet("hyb build alloc")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertIn("run wl-hy-load", trace)
        self.assertNotIn("run wl-hy-alloc", trace)

    def test_checker_failure_stops_before_building(self):
        result, trace = self.run_triplet("hyb check load")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertNotIn("hyb build load", trace)

    def test_go_build_failure_stops_before_rust(self):
        result, trace = self.run_triplet("go")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertNotIn("cargo", trace)

    def test_rust_build_failure_stops_before_hybrid(self):
        result, trace = self.run_triplet("cargo")
        self.assertEqual(result.returncode, 17, result.stdout + result.stderr)
        self.assertNotIn("hyb check load", trace)

    def test_preexisting_scratch_data_is_not_deleted_or_overwritten(self):
        existing = self.root / "gowl/user-data"
        existing.parent.mkdir()
        existing.write_text("preserve this directory")
        previous = self.root / "wl-go"
        previous.write_text("preserve this file")
        result, _ = self.run_triplet()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(existing.exists(), "runner deleted preexisting scratch data")
        self.assertEqual(existing.read_text(), "preserve this directory")
        self.assertEqual(previous.read_text(), "preserve this file")

    def test_scratch_outputs_are_cleaned_after_success_or_failure(self):
        for failure in ("", "hyb build load", "go"):
            with self.subTest(failure=failure):
                result, _ = self.run_triplet(failure)
                self.assertEqual(result.returncode, 17 if failure else 0)
                self.assertEqual(list(self.scratch.iterdir()), [])
                for name in ("gowl", "wl-go", "wl-hy-load", "wl-hy-alloc"):
                    self.assertFalse((self.root / name).exists(), name)

    def test_overlapping_runs_keep_the_first_build_workspace(self):
        ready = self.root / "go-ready"
        release = self.root / "go-release"
        first_env = self.triplet_env()
        first_env.update(GO_READY=str(ready), GO_RELEASE=str(release))
        first = subprocess.Popen(
            ["bash", str(self.runner)], env=first_env, cwd=self.root,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            deadline = time.monotonic() + 5
            while (not ready.exists() and first.poll() is None
                   and time.monotonic() < deadline):
                time.sleep(0.01)
            self.assertTrue(ready.exists(), "first Go builder did not reach its barrier")
            second, _ = self.run_triplet()
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
        finally:
            release.touch()
            try:
                stdout, stderr = first.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                first.kill()
                first.communicate()
                raise
        self.assertEqual(first.returncode, 0, stdout + stderr)


class RustTwinSharedTargetDirTests(unittest.TestCase):
    """Issue #4894: with a shared CARGO_TARGET_DIR, the old runner built the
    Rust twin via `cargo build` and then separately exec'd the crate-local
    ./target/release/workloads path. That path is independent of
    CARGO_TARGET_DIR, so it either 127'd (clean checkout) or re-ran whatever
    binary was last left sitting there (stale checkout). The fix runs the
    crate through `cargo run`, which always launches the binary cargo just
    built, wherever CARGO_TARGET_DIR points it.

    This pulls the live Rust-twin line out of run.sh (instead of
    hardcoding the fixed command) so a regression back to the
    build-then-exec pattern fails the test even if the surrounding script
    changes shape.
    """

    def setUp(self):
        if not shutil.which("cargo"):
            self.skipTest("needs cargo")
        script = (ROOT / "benchmarks/workloads/run.sh").read_text()
        match = re.search(r'echo "== Rust twin =="\n(.+)\n', script)
        self.assertIsNotNone(match, "could not find the Rust twin line in run.sh")
        self.rust_twin_cmd = match.group(1)
        self.assertNotIn(
            "./target/release/workloads",
            self.rust_twin_cmd,
            "run.sh must launch the Rust twin through `cargo run`, not a "
            "separate exec of a crate-relative target path (issue #4894)",
        )

        temporary = tempfile.TemporaryDirectory(prefix="hybrid-rust-twin-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.workloads = self.root / "benchmarks/workloads"
        rust_dir = self.workloads / "rust"
        shutil.copytree(ROOT / "benchmarks/workloads/rust/src", rust_dir / "src")
        shutil.copy(ROOT / "benchmarks/workloads/rust/Cargo.toml", rust_dir / "Cargo.toml")
        self.rust_dir = rust_dir
        self.env = dict(os.environ, CARGO_TARGET_DIR=str(self.root / "shared-target"))

    def run_rust_twin(self):
        return subprocess.run(
            ["bash", "-c", self.rust_twin_cmd], cwd=self.workloads, env=self.env,
            capture_output=True, text=True, timeout=120,
        )

    def test_ignores_stale_binary_left_at_crate_relative_target_path(self):
        stale = self.rust_dir / "target/release/workloads"
        stale.parent.mkdir(parents=True)
        stale.write_text("#!/bin/sh\necho obsolete-benchmark\n")
        stale.chmod(0o755)

        result = self.run_rust_twin()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("obsolete-benchmark", result.stdout)
        self.assertRegex(result.stdout, r"load p50=\d+ns")

    def test_runs_with_a_clean_shared_target_dir(self):
        result = self.run_rust_twin()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertRegex(result.stdout, r"load p50=\d+ns")


if __name__ == "__main__":
    unittest.main()
