#!/usr/bin/env python3
"""test_shepherd_gate.py — offline self-tests for tools/shepherd.py.

No network, no subprocesses: only the pure functions that the workflows lean
on. If the kill-switch matrix, the dup planner, the conflict classifier, or
the probe parser drift, CI fails before any workflow can.
"""

import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import shepherd as sh  # noqa: E402


class GateMatrix(unittest.TestCase):
    """The kill switch is the safety floor for every workflow (plan §1.5)."""

    def test_off_blocks_everything(self):
        for req in ("read", "dry-run", "live"):
            self.assertFalse(sh.mode_allows("off", req), req)

    def test_dry_run_allows_read_and_comment_only(self):
        self.assertTrue(sh.mode_allows("dry-run", "read"))
        self.assertTrue(sh.mode_allows("dry-run", "dry-run"))
        self.assertFalse(sh.mode_allows("dry-run", "live"))

    def test_live_allows_all(self):
        for req in ("read", "dry-run", "live"):
            self.assertTrue(sh.mode_allows("live", req), req)

    def test_unknown_mode_falls_back_to_dry_run(self):
        self.assertTrue(sh.mode_allows("bogus", "read"))
        self.assertFalse(sh.mode_allows("bogus", "live"))


def _item(n, **kw):
    it = {"number": n, "wildcard_hold": False, "draft_parked": False,
          "dup_members": None, "dup_of": None}
    it.update(kw)
    return it


class DupPlanner(unittest.TestCase):
    """W4 Phase A must never propose against closed survivors, wildcard
    holds, or drafts (the [474, 616, 945] and [620, 910] lessons)."""

    def test_open_cluster_planned(self):
        items = [_item(5, dup_members=[6, 7]), _item(6), _item(7)]
        clusters = [[5, 6, 7]]
        plan = sh.plan_phase_a(items, clusters, is_open=lambda n: n in (5, 6, 7))
        self.assertEqual(plan, [(5, [6, 7])])

    def test_closed_survivor_skips_cluster(self):
        items = [_item(620, dup_members=[910]), _item(910)]
        clusters = [[620, 910]]
        plan = sh.plan_phase_a(items, clusters, is_open=lambda n: n == 910)
        self.assertEqual(plan, [])

    def test_wildcard_survivor_and_members_skipped(self):
        items = [_item(616, dup_members=[474, 945], wildcard_hold=True),
                 _item(474, wildcard_hold=True), _item(945)]
        clusters = [[474, 616, 945]]
        plan = sh.plan_phase_a(items, clusters, is_open=lambda n: True)
        self.assertEqual(plan, [])

    def test_wildcard_member_filtered_leaves_no_pairs(self):
        # cluster [384, 572]: survivor 572 open, member 384 wildcard-held ->
        # the only member is filtered out, so the cluster yields nothing.
        items = [_item(572, dup_members=[384]), _item(384, wildcard_hold=True)]
        clusters = [[384, 572]]
        plan = sh.plan_phase_a(items, clusters, is_open=lambda n: True)
        self.assertEqual(plan, [])


class ConflictClassifier(unittest.TestCase):
    """Docs-only conflicts resolve deterministically; anything else is human
    territory (the #870 policy, plan §3 W5)."""

    def test_docs_only(self):
        docs, real = sh.classify_conflicts(["PLAN.md", "STATUS.md"],
                                           {"PLAN.md", "STATUS.md"})
        self.assertEqual(docs, ["PLAN.md", "STATUS.md"])
        self.assertEqual(real, [])

    def test_any_code_conflict_is_real(self):
        docs, real = sh.classify_conflicts(["PLAN.md", "src/hycheck/buildable.rs"],
                                           {"PLAN.md", "STATUS.md"})
        self.assertEqual(docs, ["PLAN.md"])
        self.assertEqual(real, ["src/hycheck/buildable.rs"])


class ProbeParser(unittest.TestCase):
    """W6 parses added diff lines only, and matches the #804 registry entry."""

    DIFF = ("diff --git a/x.hy b/x.hy\n"
            "--- a/x.hy\n"
            "+++ b/x.hy\n"
            "@@ -1,3 +1,4 @@\n"
            " keep me\n"
            "-removed line with make(<-chan int)\n"
            "+let c = make(<-chan int, 1)\n"
            "+++ not an added line\n")

    def test_added_lines_skip_context_removed_and_file_headers(self):
        # '+'-lines only: context, '-'-lines, and '+++' file headers are not
        # additions (a removed line proving a premise does not count).
        lines = list(sh.added_lines(self.DIFF))
        self.assertEqual(lines, ["let c = make(<-chan int, 1)"])

    def test_registry_matches_added_only(self):
        rule = sh.PROBES[0]
        added = list(sh.added_lines(self.DIFF))
        self.assertTrue(any(re.search(rule["pattern"], ln) for ln in added))
        self.assertFalse(any("removed line" in ln for ln in added))


import contextlib
from datetime import datetime, timedelta, timezone
import io
from unittest.mock import patch
import queue_sweeper


class QueueSweeperLimitsTests(unittest.TestCase):
    def run_cli(self, args, default_age=5, fresh=False):
        calls = []

        def api(path, method="GET", body=None):
            calls.append((path, method, body))
            if method == "PATCH":
                return 200, {}
            if method == "POST":
                return 201, {}
            if "/pulls?" in path:
                return 200, [
                    {
                        "number": n,
                        "created_at": (
                            (datetime.now(timezone.utc) - timedelta(hours=1))
                            .isoformat()
                            .replace("+00:00", "Z")
                            if fresh
                            else "2020-01-01T00:00:00Z"
                        ),
                        "labels": [],
                        "user": {"login": "fixture"},
                        "title": "fixture",
                    }
                    for n in (1, 2, 3)
                ]
            return 200, {}

        with patch.object(queue_sweeper, "api", side_effect=api), \
             patch.object(queue_sweeper, "TOK", "offline-fixture"), \
             patch.object(queue_sweeper, "MIN_AGE", default_age), \
             patch.object(queue_sweeper, "config_protected_numbers", return_value=set()), \
             patch.object(queue_sweeper.time, "sleep"), \
             patch("sys.argv", ["queue_sweeper.py", *args]), \
             contextlib.redirect_stdout(io.StringIO()), \
             contextlib.redirect_stderr(io.StringIO()):
            try:
                queue_sweeper.main()
                status = 0
            except SystemExit as e:
                status = e.code
        return status, calls

    def test_negative_limits_abort_before_any_api_call(self):
        for args in (["--max", "-1"], ["--max", "-200"], ["--min-age", "-1"]):
            with self.subTest(args=args):
                status, calls = self.run_cli(args, fresh=(args[0] == "--min-age"))
                self.assertEqual(status, 2)
                self.assertEqual(calls, [])

    def test_negative_environment_default_is_also_rejected(self):
        status, calls = self.run_cli([], default_age=-1, fresh=True)
        self.assertEqual(status, 2)
        self.assertEqual(calls, [])

    def test_zero_max_makes_no_provider_mutations(self):
        status, calls = self.run_cli(["--max", "0"])
        self.assertEqual(status, 0)
        self.assertFalse(any(method != "GET" for _, method, _ in calls))

    def test_positive_max_limits_number_of_closures(self):
        status, calls = self.run_cli(["--max", "1"])
        self.assertEqual(status, 0)
        self.assertEqual(sum(method == "PATCH" for _, method, _ in calls), 1)
        self.assertEqual(sum(method == "POST" for _, method, _ in calls), 1)

    def test_explicit_zero_age_remains_supported(self):
        status, calls = self.run_cli(["--min-age", "0", "--max", "1"])
        self.assertEqual(status, 0)
        self.assertEqual(sum(method == "PATCH" for _, method, _ in calls), 1)


if __name__ == "__main__":

    unittest.main()
