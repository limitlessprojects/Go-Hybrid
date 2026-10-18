"""Offline failure-propagation checks for the workload triplet runner."""
from pathlib import Path
import os
import subprocess
import sys
import tempfile
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

        # Relocate only the runner's fixed /tmp outputs: no host-wide scratch
        # files are changed, and its command/errexit control flow is untouched.
        script = (ROOT / "benchmarks/workloads/run.sh").read_text()
        for name in ("gowl", "wl-go", "wl-hy-load", "wl-hy-alloc"):
            script = script.replace("/tmp/" + name, str(self.root / name))
        self.runner = self.workloads / "run.sh"
        self.runner.write_text(script)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        stub = "#!" + sys.executable + "\n" + '''
import os
from pathlib import Path
import sys

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

    def run_triplet(self, failure=""):
        env = {
            "PATH": str(self.bin) + os.pathsep + os.defpath,
            "HOME": str(self.root / "home"),
            "HYB": str(self.bin / "hyb"),
            "TRACE": str(self.trace),
            "FAIL_STAGE": failure,
        }
        result = subprocess.run(
            ["bash", str(self.runner)], env=env, cwd=self.root,
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


if __name__ == "__main__":
    unittest.main()
