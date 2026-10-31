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




class StateLabelReconciliation(unittest.TestCase):
    """#986: bot:conflict / bot:stale-base / bot:land-candidate must be
    reconciled so a PR that resolves its conflict loses stale bucket labels."""

    def test_label_plan_removes_stale_conflict_when_now_land_candidate(self):
        add, remove = sh.label_plan(
            "LAND-CANDIDATE",
            ["bot:conflict", "bot:stale-base", "bot:verified"],
        )
        self.assertEqual(add, ["bot:land-candidate"])
        self.assertEqual(remove, ["bot:conflict", "bot:stale-base"])

    def test_label_plan_noop_when_already_matching(self):
        add, remove = sh.label_plan(
            "LAND-CANDIDATE",
            ["bot:land-candidate", "bot:ready-to-land"],
        )
        self.assertEqual(add, [])
        self.assertEqual(remove, [])

    def test_label_plan_without_existing_labels_posts_want(self):
        add, remove = sh.label_plan("CONFLICTS", None)
        self.assertEqual(add, ["bot:conflict"])
        self.assertEqual(remove, [])


class TriageCollectQuotaTests(unittest.TestCase):
    """#1103: collect() skips compare when base.sha == main_sha and caches
    merge_base -> main_paths across PRs sharing the same merge_base."""

    def test_collect_skips_up_to_date_compare_and_caches_merge_base(self):
        import types
        import triage_bot as tb

        calls = []

        class FakeGh:
            repo = "limitlessprojects/Go-Hybrid"

            def paginate(self, path):
                calls.append(("PAGINATE", path))
                if path.startswith("/repos/limitlessprojects/Go-Hybrid/pulls?"):
                    return [
                        {
                            "number": 101,
                            "title": "PR up to date with main",
                            "user": {"login": "alice"},
                            "created_at": "2026-10-25T00:00:00Z",
                            "updated_at": "2026-10-25T01:00:00Z",
                            "draft": False,
                            "head": {"sha": "aaaa1111aaaa1111"},
                            "base": {"sha": "mainsha1234567890"},
                            "labels": [{"name": "bot:conflict"}],
                        },
                        {
                            "number": 102,
                            "title": "PR behind main #1",
                            "user": {"login": "bob"},
                            "created_at": "2026-10-25T00:00:00Z",
                            "updated_at": "2026-10-25T01:00:00Z",
                            "draft": False,
                            "head": {"sha": "bbbb2222bbbb2222"},
                            "base": {"sha": "oldbase000000000"},
                            "labels": [],
                        },
                        {
                            "number": 103,
                            "title": "PR behind main #2 sharing merge_base",
                            "user": {"login": "carol"},
                            "created_at": "2026-10-25T00:00:00Z",
                            "updated_at": "2026-10-25T01:00:00Z",
                            "draft": False,
                            "head": {"sha": "cccc3333cccc3333"},
                            "base": {"sha": "oldbase000000000"},
                            "labels": [],
                        },
                    ]
                if "/files?" in path:
                    return [{"filename": "tools/shepherd.py", "additions": 5, "deletions": 1}]
                return []

            def get(self, path):
                calls.append(("GET", path))
                if path.endswith("/commits/main"):
                    return {"sha": "mainsha1234567890"}
                if "/pulls/" in path:
                    num = int(path.rsplit("/", 1)[-1])
                    return {
                        "number": num,
                        "mergeable": True,
                        "mergeable_state": "clean",
                        "labels": [{"name": "bot:conflict"}] if num == 101 else [],
                    }
                if "compare/mainsha1234567890..." in path:
                    return {
                        "behind_by": 12,
                        "merge_base_commit": {"sha": "sharedmb999999"},
                    }
                if "compare/sharedmb999999...mainsha1234567890" in path:
                    return {"files": [{"filename": "tools/shepherd.py"}]}
                return {}

        args = types.SimpleNamespace(jobs=1)
        items, main_sha = tb.collect({}, FakeGh(), args)
        self.assertEqual(main_sha, "mainsha1234567890")
        by_num = {it["number"]: it for it in items}
        self.assertEqual(by_num[101]["behind_by"], 0)
        self.assertEqual(by_num[101]["existing_labels"], ["bot:conflict"])
        # PR #101 must NOT trigger compare/mainsha...aaaa1111
        self.assertFalse(any("aaaa1111" in p for _, p in calls))
        # Shared merge_base compare must be fetched only ONCE across PR #102 and #103
        mb_compares = [p for _, p in calls if "compare/sharedmb999999...mainsha1234567890" in p]
        self.assertEqual(len(mb_compares), 1)


if __name__ == "__main__":
    unittest.main()
