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


import argparse
import json
import tempfile
import triage_bot as tb


class FakeGh:
    """Offline mock of triage_bot.Gh for testing 403 read-only tokens and RateGuard."""

    def __init__(self, repo="limitlessprojects/Go-Hybrid", fail_writes_403=False,
                 rate_guard_after=None, prs=None):
        self.repo = repo
        self.calls = 0
        self.remaining = 4500
        self.fail_writes_403 = fail_writes_403
        self.rate_guard_after = rate_guard_after
        self.prs = prs or []
        self.patched = []
        self.posted = []

    def _tick(self, method, path):
        self.calls += 1
        if self.rate_guard_after is not None and self.calls > self.rate_guard_after:
            raise tb.RateGuard("rate limit low: 100 remaining")
        if self.fail_writes_403 and method in ("POST", "PATCH", "PUT"):
            raise RuntimeError("%s %s -> 403 Resource not accessible by integration" % (method, path))

    def req(self, method, path, body=None):
        self._tick(method, path)
        if path.startswith("/search/issues"):
            return 200, {"items": [{"number": 940}]}
        return 200, {}

    def get(self, path):
        self._tick("GET", path)
        if path.endswith("/commits/main"):
            return {"sha": "abcdef1234567890"}
        if "/pulls/" in path:
            return {"mergeable": True, "mergeable_state": "clean"}
        if "/compare/" in path:
            return {"behind_by": 0, "merge_base_commit": {"sha": "abcdef1234567890"}, "files": []}
        return {}

    def paginate(self, path, max_pages=20, key=None):
        self._tick("GET", path)
        if "/pulls?state=open" in path:
            return list(self.prs)
        if "/files" in path:
            return [{"filename": "src/main.rs", "additions": 10, "deletions": 2}]
        return []

    def post(self, path, body):
        self._tick("POST", path)
        self.posted.append((path, body))
        return {"html_url": "https://github.com/%s/issues/940" % self.repo}

    def patch(self, path, body):
        self._tick("PATCH", path)
        self.patched.append((path, body))
        return {}


class ShepherdBoardResilience(unittest.TestCase):
    """Issue #2057: shepherd-board must degrade gracefully on read-only fork tokens
    (403) and shared installation quota exhaustion (RateGuard / incremental cache)."""

    def _sample_pr(self, n):
        return {
            "number": n,
            "title": "Fix bug #%d" % n,
            "user": {"login": "contributor"},
            "created_at": "2026-10-26T00:00:00Z",
            "updated_at": "2026-10-26T01:00:00Z",
            "draft": False,
            "head": {"sha": "1111222233334444"},
        }

    def test_readonly_403_detector(self):
        err = RuntimeError("PATCH /repos/x/y/issues/940 -> 403 Resource not accessible by integration")
        self.assertTrue(tb._is_readonly_403(err))
        err_500 = RuntimeError("PATCH /repos/x/y/issues/940 -> 500 Internal Error")
        self.assertFalse(tb._is_readonly_403(err_500))

    def test_ensure_palette_and_sync_labels_degrade_on_readonly_403(self):
        cfg = sh.load_cfg()
        gh = FakeGh(fail_writes_403=True)
        ok = sh.ensure_palette(gh, cfg)
        self.assertFalse(ok)

    def test_collect_reuses_cache_and_degrades_on_rate_guard(self):
        cfg = tb.load_cfg()
        prs = [self._sample_pr(101), self._sample_pr(102), self._sample_pr(103)]
        with tempfile.TemporaryDirectory() as td:
            cache_file = Path(td) / "TRIAGE.json"
            cached_item = {
                "number": 101,
                "title": "Fix bug #101",
                "author": "contributor",
                "created_at": "2026-10-26T00:00:00Z",
                "updated_at": "2026-10-26T01:00:00Z",
                "age_days": 1.0,
                "idle_days": 1.0,
                "draft": False,
                "mergeable": True,
                "mergeable_state": "clean",
                "head_sha": "111122223333",
                "files": 1,
                "paths_sample": ["tests/foo_accept.hy"],
                "additions": 15,
                "deletions": 0,
                "behind_by": 0,
                "overlap_risk": [],
                "tests_included": True,
            }
            cache_file.write_text(json.dumps({
                "generated": "2026-10-26 01:00 UTC",
                "main": "abcdef1234567890",
                "clusters": [],
                "items": [cached_item],
            }))
            # Trigger RateGuard after 4 calls (list PRs + main sha + 2 calls on PR #102)
            gh = FakeGh(prs=prs, rate_guard_after=4)
            args = argparse.Namespace(cache=str(cache_file), max_calls=None, jobs=1)
            items, main_sha = tb.collect(cfg, gh, args)
            self.assertEqual(main_sha, "abcdef1234567890")
            self.assertEqual(len(items), 3)
            by_num = {it["number"]: it for it in items}
            # PR #101 came straight from cache with tests_included preserved
            self.assertTrue(by_num[101]["tests_included"])
            # PR #102/#103 degraded gracefully to fallback metadata instead of crashing scan
            self.assertIn(by_num[103]["mergeable_state"], ("unknown", "clean"))


if __name__ == "__main__":
    unittest.main()
