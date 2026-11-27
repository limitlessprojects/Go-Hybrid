#!/usr/bin/env python3
"""test_triage_bot.py — unit tests for tools/triage_bot.py test detection and ranking."""

import os
import sys
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parent.parent
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))
TOOLS = Path(__file__).resolve().parent
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

try:
    from triage_bot import has_tests
except ImportError:
    from tools.triage_bot import has_tests


class TestTriageBotTestDetection(unittest.TestCase):
    def test_standard_test_directories_and_fixtures(self):
        self.assertTrue(has_tests(["tests/checker-corpus/01-bad.hy"]))
        self.assertTrue(has_tests(["tests/suite/basic.hy"]))
        self.assertTrue(has_tests(["corpus/example.hy"]))
        self.assertTrue(has_tests(["fixtures/repro_accept.hy"]))
        self.assertTrue(has_tests(["fixtures/repro_reject.hy"]))

    def test_go_and_rust_test_files(self):
        self.assertTrue(has_tests(["pkg/emitter/emit_test.go"]))
        self.assertTrue(has_tests(["crates/hy-check/src/checker_test.rs"]))
        self.assertTrue(has_tests(["compiler/crates/hy-emit/src/tests.rs"]))

    def test_repository_tools_test_suites(self):
        # Tools test files that were previously missed by triage_bot
        self.assertTrue(has_tests(["tools/check_guarantees.py", "tools/test_check_guarantees.py"]))
        self.assertTrue(has_tests(["tools/runner/test_runner.py"]))
        self.assertTrue(has_tests(["tools/fuzz/test_hyfuzz.py"]))
        self.assertTrue(has_tests(["tools/test_shepherd_gate.py"]))
        self.assertTrue(has_tests(["tools/test_differential_reject_rule.py"]))

    def test_non_test_files_rejected(self):
        self.assertFalse(has_tests(["tools/check_guarantees.py"]))
        self.assertFalse(has_tests(["tools/shepherd.py"]))
        self.assertFalse(has_tests(["tools/triage_bot.py"]))
        self.assertFalse(has_tests(["compiler/crates/hy-syntax/src/parser.rs"]))
        # False-positive guard: "latest_report.py" contains "test_" substring ("latest_")
        # but is not a test suite.
        self.assertFalse(has_tests(["tools/latest_report.py"]))


if __name__ == "__main__":
    unittest.main()
