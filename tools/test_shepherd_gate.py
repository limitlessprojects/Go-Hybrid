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
import io
from unittest.mock import patch
import queue_sweeper as qs


class QueueSweeperStatus(unittest.TestCase):
    """Accurate accounting of PATCH closure and comment delivery responses (fixes #2107)."""

    def setUp(self):
        self.sample_pr = {
            "number": 101,
            "created_at": "2020-01-01T00:00:00Z",
            "labels": [],
            "user": {"login": "test-contributor"},
            "title": "Old PR",
        }

    def _run_sweeper(self, patch_status, comment_status, get_pr_state=None):
        calls = []

        def mock_api(path, method="GET", body=None):
            calls.append({"path": path, "method": method, "body": body})
            if "/pulls?" in path:
                return 200, [self.sample_pr]
            if method == "PATCH":
                return patch_status, None
            if method == "POST":
                return comment_status, None
            if method == "GET" and f"/pulls/{self.sample_pr['number']}" in path:
                return 200, {"state": get_pr_state or "open"}
            return 200, {"open_issues_count": 1}

        out = io.StringIO()
        with patch.object(qs, "TOK", "offline-mock-token"), \
             patch.object(qs, "api", mock_api), \
             patch.object(qs, "config_protected_numbers", return_value=set()), \
             patch.object(qs.time, "sleep"), \
             patch.object(sys, "argv", ["queue_sweeper.py"]), \
             contextlib.redirect_stdout(out):
            qs.main()

        return out.getvalue(), calls

    def test_successful_close_and_comment(self):
        output, _ = self._run_sweeper(200, 201)
        self.assertIn("closed=1 fails=0 comment_fails=0", output)

    def test_rejected_patch_422_counted_as_fail_when_still_open(self):
        output, _ = self._run_sweeper(422, 201, get_pr_state="open")
        self.assertIn("closed=0 fails=1 comment_fails=0", output)

    def test_patch_422_confirmed_closed_via_get(self):
        output, _ = self._run_sweeper(422, 201, get_pr_state="closed")
        self.assertIn("closed=1 fails=0 comment_fails=0", output)

    def test_comment_failure_tracked_separately_without_failing_close(self):
        output, _ = self._run_sweeper(200, 422)
        self.assertIn("closed=1 fails=0 comment_fails=1", output)

    def test_patch_server_error_500(self):
        output, _ = self._run_sweeper(500, 201)
        self.assertIn("closed=0 fails=1 comment_fails=0", output)


if __name__ == "__main__":
    unittest.main()
