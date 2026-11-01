"""Offline environment-report probes using real executable command shims.

No host tools, hardware reports, cloud service, credentials or private environment
are queried. Only version probes reach the runner's real subprocess function.
"""
import importlib.util
import os
from pathlib import Path
import shlex
import tempfile
import unittest
from unittest import mock

RUNNER = Path(os.environ.get("RUNNER_PATH", str(Path(__file__).parent / "runner" / "runner.py")))
SPEC = importlib.util.spec_from_file_location("runner_under_test", RUNNER)
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)
TOOLS = ("go", "rustc", "cargo", "gcc", "clang", "git", "cc")


class EnvironmentReportTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="runner-version-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "arguments.log"
        self.env = {"PATH": str(self.bin), "HOME": str(self.root), "LC_ALL": "C"}
        # The baseline uses head; this shim makes that real pipeline portable
        # without finding any host executable via PATH.
        self.executable("head", 'IFS= read -r line || :\nprintf "%s\\n" "$line"\n')
        for tool in TOOLS:
            expected = "version" if tool == "go" else "--version"
            self.tool(tool, self.version(tool), expected=expected)
        self.real_run_cmd = runner.run_cmd

    @staticmethod
    def version(tool):
        return "go version go1.26.9 darwin/arm64" if tool == "go" else tool + " fixture-version"

    def executable(self, name, body):
        path = self.bin / name
        path.write_text("#!/bin/sh\n" + body)
        path.chmod(0o700)

    def tool(self, name, output, status=0, expected=None):
        body = "printf '%s\\n' " + shlex.quote(name + ":") + '"$*" >> ' + shlex.quote(str(self.log)) + "\n"
        if expected is not None:
            body += 'if [ "$#" -ne 1 ] || [ "$1" != ' + shlex.quote(expected) + ' ]; then\n  printf "unsupported argument\\n" >&2\n  exit 2\nfi\n'
        body += "printf '%s' " + shlex.quote(output) + "\nexit " + str(status) + "\n"
        self.executable(name, body)

    def report(self):
        def bounded_probe(command, timeout, cwd=None, env=None):
            if isinstance(command, list) and command and command[0] in TOOLS:
                return self.real_run_cmd(command, timeout, cwd=self.root, env=self.env)
            if isinstance(command, str) and command.split(" ", 1)[0] in TOOLS:
                return self.real_run_cmd(command, timeout, cwd=self.root, env=self.env)
            return 0, 0.0, "hardware fixture"
        with mock.patch.object(runner, "run_cmd", side_effect=bounded_probe), mock.patch.dict(os.environ, self.env, clear=True), mock.patch.object(runner.platform, "platform", return_value="test-platform"), mock.patch.object(runner.platform, "machine", return_value="test-machine"):
            report = runner.env_report()
        self.assertEqual(report["kaggle_env"], {})
        self.assertEqual(report["ci_env"], {})
        return report

    def test_go_uses_supported_version_subcommand(self):
        report = self.report()
        self.assertEqual(report["tool_go"], self.version("go"))
        self.assertIn("go:version", self.log.read_text().splitlines())

    def test_other_tools_keep_double_dash_version_and_first_line(self):
        for tool in TOOLS[1:]:
            self.tool(tool, self.version(tool) + "\nextra detail\n", expected="--version")
        report = self.report()
        for tool in TOOLS[1:]:
            with self.subTest(tool=tool):
                self.assertEqual(report["tool_" + tool], self.version(tool))
                self.assertIn(tool + ":--version", self.log.read_text().splitlines())

    def test_nonzero_exit_does_not_turn_output_into_valid_version(self):
        for tool in TOOLS:
            self.tool(tool, self.version(tool) + "\n", status=7)
        report = self.report()
        for tool in TOOLS:
            with self.subTest(tool=tool):
                self.assertEqual(report["tool_" + tool], "n/a")

    def test_empty_success_is_unavailable(self):
        for tool in TOOLS:
            self.tool(tool, "")
        report = self.report()
        for tool in TOOLS:
            with self.subTest(tool=tool):
                self.assertEqual(report["tool_" + tool], "n/a")

    def test_missing_tool_is_unavailable(self):
        (self.bin / "go").unlink()
        self.assertEqual(self.report()["tool_go"], "n/a")

    def test_unexpected_go_output_is_unavailable(self):
        self.tool("go", "unexpected output\n", expected="version")
        self.assertEqual(self.report()["tool_go"], "n/a")

    def test_go_development_version_remains_reportable(self):
        value = "go version devel go1.27-abcdef linux/amd64"
        self.tool("go", value + "\n", expected="version")
        self.assertEqual(self.report()["tool_go"], value)


    def test_autoland_missing_bundle_files_returns_no_bundle(self):
        from tools import autoland
        bdir = self.root / "incomplete-bundle"
        bdir.mkdir()
        (bdir / "meta.json").write_text('{"issue": 1, "expected_code": "HYB-E1000"}\n')
        with mock.patch.object(autoland, "QUEUE", self.root):
            self.assertEqual(autoland.run_bundle("incomplete-bundle", False, "GITHUB_TOKEN"), 2)


if __name__ == "__main__":
    unittest.main()
