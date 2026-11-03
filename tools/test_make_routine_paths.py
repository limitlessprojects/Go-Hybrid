"""Exercise real Make recipes with owned scratch and offline tool stand-ins."""
from pathlib import Path
import json
import os
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
MAKE = shutil.which("make")

RUNNER = r'''
from pathlib import Path
import json
import os
import sys
import time

job = json.loads(Path(sys.argv[1]).read_text())
assert job["job"] == "selftest"
trace = Path(os.environ["SELFTEST_TRACE"])
trace.write_text(json.dumps({"cwd": str(Path.cwd()), "args": sys.argv[1:]}))
marker = Path(trace.name + "-owned")
marker.write_text("keep this invocation's input")
if os.environ.get("SELFTEST_READY"):
    Path(os.environ["SELFTEST_READY"]).touch()
    deadline = time.monotonic() + 10
    while not Path(os.environ["SELFTEST_RELEASE"]).exists():
        if time.monotonic() > deadline:
            sys.exit(24)
        time.sleep(0.01)
    if not marker.exists():
        sys.exit(23)
if os.environ.get("SELFTEST_FAIL"):
    sys.exit(17)
results = Path(sys.argv[sys.argv.index("--results-dir") + 1])
results.mkdir()
(results / "summary.json").write_text(json.dumps({"ok": True}))
'''

TOOL = r'''
from pathlib import Path
import json
import os
import sys
with open(os.environ["MAKE_TOOL_TRACE"], "a") as trace:
    trace.write(json.dumps({
        "tool": Path(sys.argv[0]).name,
        "args": sys.argv[1:],
        "target": os.environ.get("CARGO_TARGET_DIR"),
        "cwd": str(Path.cwd()),
    }) + "\n")
'''


@unittest.skipUnless(MAKE and Path("/bin/sh").is_file(), "requires POSIX make/sh")
class MakeRoutinePathTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-make-paths-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.scratch = self.root / "temporary files"
        self.scratch.mkdir()

    def fixture(self, source_name="source"):
        source = self.root / source_name
        (source / "tools/runner").mkdir(parents=True)
        (source / "compiler").mkdir()
        (source / "tools/runner/runner.py").write_text(RUNNER)
        return source

    def selftest_makefile(self, source):
        lines = (ROOT / "Makefile").read_text().splitlines()
        pivot = next(i for i, line in enumerate(lines) if '"job":"selftest"' in line)
        start, end = pivot, pivot
        while start and lines[start - 1].rstrip().endswith("\\"):
            start -= 1
        while lines[end].rstrip().endswith("\\"):
            end += 1
        recipe = "\n".join(lines[start:end + 1])
        # Relocate ONLY the legacy fixed scratch root. A baseline reproduction
        # must never remove the host's /tmp/gh-selftest. The fixed recipe uses
        # the owned TMPDIR normally and needs no transformation.
        recipe = recipe.replace(
            "cd /tmp &&",
            "cd " + shlex.quote(str(self.scratch)) + " &&",
        )
        self.assertNotIn("cd /tmp", recipe)
        makefile = source / "Selftest.mk"
        makefile.write_text(
            "SHELL := /bin/sh\nPY := " + shlex.quote(sys.executable)
            + "\n.PHONY: selftest\nselftest:\n" + recipe + "\n"
        )
        return [MAKE, "--no-print-directory", "-f", str(makefile), "selftest"]

    def environment(self, trace_name, **extra):
        env = os.environ.copy()
        env.update(TMPDIR=str(self.scratch),
                   SELFTEST_TRACE=str(self.root / trace_name), **extra)
        return env

    def run_make(self, command, source, env):
        return subprocess.run(command, cwd=source, env=env, text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              timeout=15)

    def test_checkout_path_with_spaces_reaches_runner(self):
        source = self.fixture("source with spaces")
        result = self.run_make(self.selftest_makefile(source), source,
                               self.environment("spaces.json"))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        observed = json.loads((self.root / "spaces.json").read_text())
        self.assertEqual(observed["args"], ["job.json", "--results-dir", "results"])
        self.assertFalse(Path(observed["cwd"]).exists(), "owned scratch leaked")

    def test_existing_scratch_survives_success_and_failure(self):
        source = self.fixture()
        command = self.selftest_makefile(source)
        legacy = self.scratch / "gh-selftest"
        legacy.mkdir()
        sentinel = legacy / "preexisting"
        for fail in ("", "1"):
            with self.subTest(failing_runner=bool(fail)):
                sentinel.write_text("caller-owned data")
                result = self.run_make(command, source, self.environment(
                    "failure.json", SELFTEST_FAIL=fail))
                self.assertEqual(result.returncode != 0, bool(fail),
                                 result.stdout + result.stderr)
                self.assertTrue(sentinel.exists(), "preexisting data was removed")
                self.assertEqual(sentinel.read_text(), "caller-owned data")
                # Check the actual runner directory, not all of TMPDIR:
                # macOS's make launcher may also create an xcrun cache there.
                observed = json.loads((self.root / "failure.json").read_text())
                self.assertFalse(Path(observed["cwd"]).exists(),
                                 "this invocation's scratch leaked")

    def test_concurrent_selftests_keep_each_others_inputs(self):
        source = self.fixture()
        command = self.selftest_makefile(source)
        ready, release = self.root / "ready", self.root / "release"
        first = subprocess.Popen(
            command, cwd=source,
            env=self.environment("first.json", SELFTEST_READY=str(ready),
                                 SELFTEST_RELEASE=str(release)),
            text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        try:
            deadline = time.monotonic() + 10
            while not ready.exists() and first.poll() is None:
                if time.monotonic() > deadline:
                    self.fail("first self-test did not reach its owned fixture")
                time.sleep(0.01)
            self.assertTrue(ready.exists(), "first self-test exited early")
            second = self.run_make(command, source, self.environment("second.json"))
            release.touch()
            stdout, stderr = first.communicate(timeout=12)
            self.assertEqual(second.returncode, 0, second.stdout + second.stderr)
            self.assertEqual(first.returncode, 0, stdout + stderr)
            first_dir = json.loads((self.root / "first.json").read_text())["cwd"]
            second_dir = json.loads((self.root / "second.json").read_text())["cwd"]
            self.assertNotEqual(first_dir, second_dir)
            self.assertFalse(Path(first_dir).exists())
            self.assertFalse(Path(second_dir).exists())
        finally:
            release.touch()
            if first.poll() is None:
                first.kill()
            first.wait(timeout=2)
            first.stdout.close()
            first.stderr.close()

    def test_cargo_and_binary_paths_stay_single_arguments(self):
        source = self.fixture()
        tools = self.root / "tool files"
        tools.mkdir()
        target = self.root / "build files"
        (target / "debug").mkdir(parents=True)
        trace = self.root / "tool-trace.jsonl"
        for executable in (tools / "cargo", target / "debug/hyb"):
            executable.write_text("#!" + sys.executable + "\n" + TOOL)
            executable.chmod(0o755)
        for filename in ("corpus_harness.py", "differential_loop.py"):
            (source / "tools" / filename).write_text(TOOL)
        cases = {
            "workspace": [("cargo", ["check", "--workspace"])],
            "corpus": [("cargo", ["build", "-p", "hyb", "--quiet"]),
                       ("corpus_harness.py", ["--bin", str(target / "debug/hyb")])],
            "bench": [("cargo", ["build", "-p", "hyb", "--quiet"]),
                      ("hyb", ["bench-overhead"])],
        }
        for goal, expected in cases.items():
            with self.subTest(goal=goal):
                trace.write_text("")
                command = [MAKE, "--no-print-directory", "-f", str(ROOT / "Makefile"),
                           goal, "CARGO=" + str(tools / "cargo"),
                           "CARGO_TARGET_DIR=" + str(target),
                           "PY=" + shlex.quote(sys.executable)]
                env = os.environ.copy()
                env["MAKE_TOOL_TRACE"] = str(trace)
                result = self.run_make(command, source, env)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                rows = [json.loads(line) for line in trace.read_text().splitlines()]
                self.assertEqual([(r["tool"], r["args"]) for r in rows], expected)
                self.assertEqual(rows[0]["target"], str(target))


if __name__ == "__main__":
    unittest.main()

