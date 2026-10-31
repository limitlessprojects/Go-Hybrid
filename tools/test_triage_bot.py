#!/usr/bin/env python3
"""test_triage_bot.py — regression tests for the scan's `tests_included`.

The scan's answer is worth +3 of the 6 points that put a PR into the Pareto
list (`triage:pareto`) the landing sessions work from, and it answers the same
question as the shepherd's W3 gate (`tools/shepherd.py:TEST_PATTERNS`). Before
issue #2505 the scan used a narrower inline path test (`"tests/" in path`),
which missed every test in the repository's own conventions — `tools/test_*.py`
(ci.yml runs one line per tool), `spikes/*/test_*.py`, and `<crate>/src/tests.rs`
(the emitter's unit-test module) — so 67 of 200 open PRs (57 the W3 gate counted
and the scan did not, 10 neither counted) scored 5 and stayed in RECON while W3
reported "tests: present" for the same diffs (issue #2508).

These tests pin (1) the path table, (2) that the scan is never narrower than the
W3 list, and (3) that a PR of that exact shape reaches LAND-CANDIDATE.
"""

import importlib.util
import json
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent


def _load(name):
    spec = importlib.util.spec_from_file_location(name, TOOLS / (name + ".py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


triage_bot = _load("triage_bot")
shepherd = _load("shepherd")


class TestsIncludedPathTable(unittest.TestCase):
    """Every convention the repository runs counts; nothing else does."""

    CASES = (
        # the repo's test conventions
        ("compiler/crates/hy-check/tests/generic_type_arg_substitution_2490.rs", True),
        ("tests/checker-corpus/01-shared-map-across-tasks.hy", True),
        ("tests/checker-corpus/foo_accept.hy", True),
        ("tests/checker-corpus/foo_reject.hy", True),
        ("tools/test_check_guarantees.py", True),
        ("tools/runner/test_runner.py", True),
        ("tools/fuzz/test_hyfuzz.py", True),
        ("spikes/w030-verdict-differential-fuzzing/test_verdict_fuzzer.py", True),
        ("compiler/crates/hy-emit/src/tests.rs", True),
        ("compiler/crates/hy-check/src/thing_test.rs", True),
        ("runtime/foo_test.go", True),
        # not tests
        ("tools/triage_bot.py", False),
        ("compiler/crates/hy-check/src/shape.rs", False),
        ("tools/latest_report.py", False),
        ("docs/testing.md", False),
        ("", False),
    )

    def test_path_table(self):
        for path, expected in self.CASES:
            with self.subTest(path=path):
                self.assertEqual(
                    triage_bot.tests_included([path]),
                    expected,
                    "%r should be counted=%s" % (path, expected),
                )

    def test_any_path_in_the_diff_is_enough(self):
        self.assertTrue(triage_bot.tests_included(
            ["tools/check_guarantees.py", "PLAN.md", "tools/test_check_guarantees.py"]))


class ScanMatchesW3Gate(unittest.TestCase):
    """The W3 gate and the scan answer the same question; they must agree
    wherever the repository actually puts tests — and where they differ, the
    difference has to be deliberate (pinned here)."""

    AGREED = (
        "tools/test_check_guarantees.py",
        "tools/runner/test_runner.py",
        "tools/fuzz/test_hyfuzz.py",
        "spikes/w030-verdict-differential-fuzzing/test_verdict_fuzzer.py",
        "compiler/crates/hy-check/src/thing_test.rs",
        "runtime/foo_test.go",
        "compiler/crates/hy-check/tests/integration.rs",
        "tests/checker-corpus/09-a3-map-slot-valid.hy",
        "tests/checker-corpus/foo_reject.hy",
    )

    @staticmethod
    def _gate(path):
        return any(pat in path for pat in shepherd.TEST_PATTERNS)

    def test_agree_on_the_repository_conventions(self):
        for path in self.AGREED:
            with self.subTest(path=path):
                self.assertTrue(self._gate(path), "W3 gate should count %r" % path)
                self.assertTrue(triage_bot.tests_included([path]),
                                "the scan should count %r" % path)

    def test_scan_does_not_inherit_the_gate_substring_false_positive(self):
        # The W3 list's bare "test_" substring matches these names; neither is a
        # test, so the scan matches on the file name instead.
        for path in ("tools/latest_report.py", "docs/testing.md"):
            with self.subTest(path=path):
                self.assertFalse(triage_bot.tests_included([path]),
                                 "%r is not a test" % path)

    def test_scan_counts_the_unit_test_module_the_gate_misses(self):
        # `<crate>/src/tests.rs` is the emitter's unit-test module; the W3 list
        # has no pattern for it, the scan does.
        path = "compiler/crates/hy-emit/src/tests.rs"
        self.assertFalse(self._gate(path))
        self.assertTrue(triage_bot.tests_included([path]))


class ScanScoring(unittest.TestCase):
    """A PR that ships one of those tests reaches the Pareto list."""

    def setUp(self):
        self.cfg = json.loads((TOOLS / "triage_config.json").read_text())

    def item(self, paths, additions=186, behind_by=5):
        return {
            "number": 999999,
            "title": "fix(tools): a small mergeable fix with its tests",
            "author": "someone",
            "created_at": "2026-10-30T00:00:00Z",
            "updated_at": "2026-10-30T00:00:00Z",
            "age_days": 1,
            "idle_days": 1,
            "draft": False,
            "current_labels": [],
            "mergeable": True,
            "mergeable_state": "behind",
            "head_sha": "0" * 12,
            "files": len(paths),
            "paths_sample": paths,
            "additions": additions,
            "deletions": 1,
            "behind_by": behind_by,
            "overlap_risk": [],
            "tests_included": triage_bot.tests_included(paths),
        }

    def test_testless_small_pr_is_recon(self):
        it = self.item(["compiler/crates/hy-check/src/shape.rs"])
        items, _ = triage_bot.classify(self.cfg, [it])
        self.assertEqual(items[0]["score"], 5)
        self.assertEqual(items[0]["bucket"], "RECON")
        self.assertNotIn("triage:pareto", items[0]["labels"])

    def test_pr_with_a_tool_or_unit_test_reaches_pareto(self):
        # The exact shape of the mis-scored class: a small, mergeable fix a few
        # commits behind whose test is tools/test_x.py or <crate>/src/tests.rs.
        for test_path in ("tools/test_check_guarantees.py",
                          "compiler/crates/hy-emit/src/tests.rs"):
            with self.subTest(test=test_path):
                it = self.item(["tools/check_guarantees.py", test_path])
                items, _ = triage_bot.classify(self.cfg, [it])
                self.assertEqual(items[0]["score"], 8)
                self.assertEqual(items[0]["bucket"], "LAND-CANDIDATE")
                self.assertIn("triage:tests", items[0]["labels"])
                self.assertIn("triage:pareto", items[0]["labels"])


if __name__ == "__main__":
    unittest.main()
