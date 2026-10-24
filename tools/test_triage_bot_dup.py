#!/usr/bin/env python3
"""Regression tests for tools/triage_bot.py duplicate clustering (#979).

Two properties are verified:
1. Two PRs editing the same files but disjoint hunks must not cluster.
2. When PRs do cluster, the primary is the earliest-created reporter, not the
   one with the most additions.
"""

import unittest
import tools.triage_bot as tb

CFG = {
    "wildcard_holds": [],
    "drafts_parked": [],
    "dup_jaccard": 0.6,
    "behind_threshold": 15,
    "new_arrival_days": 3,
    "mega_additions": 2000,
}

SRC = "compiler/crates/hy-emit/src/stmt.rs"
LIB = "compiler/crates/hy-emit/src/lib.rs"
TST = "compiler/crates/hy-emit/src/tests.rs"


def item(number, paths, spans, created, additions=100, with_spans=True):
    return {
        "number": number,
        "title": "pr %d" % number,
        "author": "someone",
        "created_at": created,
        "updated_at": created,
        "age_days": 5,
        "idle_days": 1,
        "draft": False,
        "mergeable": True,
        "mergeable_state": "clean",
        "head_sha": "deadbeef",
        "files": len(paths),
        "paths_sample": sorted(paths),
        "additions": additions,
        "deletions": 0,
        "behind_by": 0,
        "overlap_risk": [],
        "tests_included": True,
        "hunk_spans": spans if with_spans else None,
    }


class DupPredicate(unittest.TestCase):
    def test_same_file_set_disjoint_hunks_is_not_a_duplicate(self):
        # #608 edits stmt.rs:1034; #786 edits stmt.rs:1773. Same two filenames,
        # zero shared lines -> must not cluster.
        a = item(608, [SRC, TST], {SRC: [(1034, 1042)], TST: [(1023, 1075)]},
                 "2026-10-18T00:00:00Z")
        b = item(786, [SRC, TST], {SRC: [(1773, 1790)], TST: [(2908, 2960)]},
                 "2026-10-19T00:00:00Z")
        items, clusters = tb.classify(CFG, [a, b])
        self.assertIsNone(items[0].get("dup_of"), "608 must not be a dup")
        self.assertIsNone(items[1].get("dup_of"), "786 must not be a dup")
        self.assertEqual(clusters, [], "no cluster expected")

    def test_overlapping_change_sets_still_cluster(self):
        a = item(700, [LIB, TST], {LIB: [(825, 875)], TST: [(1197, 1298)]},
                 "2026-10-19T00:00:00Z")
        b = item(701, [LIB, TST], {LIB: [(830, 870)], TST: [(1300, 1400)]},
                 "2026-10-19T01:00:00Z")
        items, clusters = tb.classify(CFG, [a, b])
        by = {i["number"]: i for i in items}
        self.assertTrue(by[700]["dup_members"], "700 should be the primary")
        self.assertEqual(by[701]["dup_of"], 700)

    def test_missing_patch_falls_back_to_path_overlap(self):
        # A PR whose patch GitHub did not return must not silently escape the
        # duplicate predicate.
        a = item(800, [SRC, TST], None, "2026-10-19T00:00:00Z", with_spans=False)
        b = item(801, [SRC, TST], None, "2026-10-19T01:00:00Z", with_spans=False)
        items, _ = tb.classify(CFG, [a, b])
        by = {i["number"]: i for i in items}
        self.assertEqual(by[801]["dup_of"], 800)

    def test_one_sided_patch_still_falls_back(self):
        a = item(810, [SRC, TST], {SRC: [(10, 20)], TST: [(5, 9)]},
                 "2026-10-19T00:00:00Z")
        b = item(811, [SRC, TST], None, "2026-10-19T01:00:00Z", with_spans=False)
        items, _ = tb.classify(CFG, [a, b])
        by = {i["number"]: i for i in items}
        self.assertEqual(by[811]["dup_of"], 810)

    def test_primary_is_the_first_reporter_not_the_biggest_diff(self):
        # BOUNTY.md: "the first reporter takes the payout". A later, much larger
        # PR must not inherit the slot.
        first = item(900, [SRC, TST], {SRC: [(100, 140)], TST: [(50, 60)]},
                     "2026-10-18T00:00:00Z", additions=60)
        later = item(901, [SRC, TST], {SRC: [(120, 200)], TST: [(55, 90)]},
                     "2026-10-19T00:00:00Z", additions=9999)
        items, _ = tb.classify(CFG, [first, later])
        by = {i["number"]: i for i in items}
        self.assertEqual(by[901]["dup_of"], 900,
                         "the later, bigger PR must not become the primary")
        self.assertEqual(by[900]["dup_members"], [901])

    def test_a_single_pr_is_never_clustered(self):
        a = item(950, [SRC, TST], {SRC: [(1, 2)], TST: [(1, 2)]},
                 "2026-10-19T00:00:00Z")
        _, clusters = tb.classify(CFG, [a])
        self.assertEqual(clusters, [])


if __name__ == "__main__":
    unittest.main()
