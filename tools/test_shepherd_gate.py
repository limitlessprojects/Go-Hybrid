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
import queue_sweeper as qs  # noqa: E402


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




class SyncLabelsTests(unittest.TestCase):
    """sync-labels must promote LAND-CANDIDATE to both bot:land-candidate
    and triage:pareto to keep land-candidates out of the 5-day sweep."""

    def test_land_candidate_promotes_to_pareto_and_bot_label(self):
        labels = sh.labels_for_item({"bucket": "LAND-CANDIDATE"})
        self.assertIn("bot:land-candidate", labels)
        self.assertIn("triage:pareto", labels)

    def test_conflicts_and_stale_base(self):
        self.assertEqual(sh.labels_for_item({"bucket": "CONFLICTS"}), ["bot:conflict"])
        self.assertEqual(sh.labels_for_item({"bucket": "STALE-BASE"}), ["bot:stale-base"])

    def test_wildcard_hold_and_draft_parked_skipped(self):
        self.assertEqual(sh.labels_for_item({"bucket": "LAND-CANDIDATE", "wildcard_hold": True}), [])
        self.assertEqual(sh.labels_for_item({"bucket": "LAND-CANDIDATE", "draft_parked": True}), [])

    def test_other_buckets_return_empty(self):
        self.assertEqual(sh.labels_for_item({"bucket": "RECON"}), [])


class QueueSweeperProtectionTests(unittest.TestCase):
    """The 5-day queue sweeper must protect land-candidates and pareto PRs."""

    def test_protected_labels_includes_land_candidate_and_pareto(self):
        self.assertIn("bot:land-candidate", qs.PROTECTED_LABELS)
        self.assertIn("triage:pareto", qs.PROTECTED_LABELS)
        self.assertIn("triage:wildcard-hold", qs.PROTECTED_LABELS)
        self.assertIn("triage:keep", qs.PROTECTED_LABELS)

    def test_is_protected_by_label(self):
        self.assertTrue(qs.is_protected({"labels": [{"name": "bot:land-candidate"}]}))
        self.assertTrue(qs.is_protected({"labels": [{"name": "triage:pareto"}]}))
        self.assertTrue(qs.is_protected({"labels": [{"name": "triage:wildcard-hold"}]}))
        self.assertTrue(qs.is_protected({"labels": [{"name": "triage:keep"}]}))

    def test_is_protected_by_config_number(self):
        self.assertTrue(qs.is_protected({"number": 999}, protected_nums={999}))

    def test_unprotected_pr(self):
        self.assertFalse(qs.is_protected({"labels": [{"name": "triage:conflicts"}], "number": 456}, protected_nums={999}))


if __name__ == "__main__":
    unittest.main()
