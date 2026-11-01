#!/usr/bin/env python3
"""test_triage_bot.py — offline unit tests for tools/triage_bot.py.

Verifies duplicate-evasion and line-golfing protections in classify() and
render_board():
  1. Copycat PR that renames a test file (Jaccard = 0.33 < 0.60) while
     targeting the same issue (#2613) or same normalized title as an earlier
     PR (#2614) is clustered into DUP of #2614, keeping #2614 (earliest PR)
     as the canonical LAND-CANDIDATE.
  2. Unrelated PRs touching the same compiler file (loans.rs) for different
     issues with different test files (Jaccard = 0.33, distinct issues/titles)
     remain independent LAND-CANDIDATEs.
  3. Pareto NOW and merge_gate order score ties by earliest PR number
     (x["number"] ascending) rather than fewest additions, neutralizing
     test-minification line-golfing.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import triage_bot as tb  # noqa: E402


def _cfg(**overrides):
    cfg = {
        "repo": "limitlessprojects/Go-Hybrid",
        "wildcard_holds": [],
        "drafts_parked": [],
        "behind_threshold": 15,
        "dup_jaccard": 0.6,
        "pareto_n": 12,
        "new_arrival_days": 3,
        "mega_additions": 2000,
    }
    cfg.update(overrides)
    return cfg


def _pr(number, title, paths, additions=80, body="", **kw):
    item = {
        "number": number,
        "title": title,
        "body": body,
        "author": "user-%d" % number,
        "created_at": "2026-10-30T00:00:00Z",
        "updated_at": "2026-10-30T00:00:00Z",
        "age_days": 1.0,
        "idle_days": 0.5,
        "draft": False,
        "current_labels": [],
        "mergeable": True,
        "mergeable_state": "clean",
        "head_sha": "%012x" % number,
        "files": len(paths),
        "paths_sample": sorted(paths)[:12],
        "issue_refs": tb.extract_issue_refs(title, body),
        "additions": additions,
        "deletions": 5,
        "behind_by": 0,
        "overlap_risk": [],
        "tests_included": any(tb.is_test_path(p) for p in paths),
    }
    item.update(kw)
    return item


class TriageBotDupAndParetoTests(unittest.TestCase):
    def test_copycat_renamed_test_same_issue_and_title_caught_as_dup_of_earlier_pr(self):
        cfg = _cfg()
        orig = _pr(
            2614,
            "fix(hy-check): clear child-origin reborrow loans in release_loans_originating_at (fixes #2613)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/reborrow_scope_exit_release.rs",
            ],
            additions=120,
            body="Fixes #2613",
        )
        copycat = _pr(
            2836,
            "fix(hy-check): clear child-origin reborrow loans in release_loans_originating_at (fixes #2613)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/reborrow_inner_origin_exit.rs",
            ],
            additions=42,
            body="Closes #2613",
        )

        # File-set Jaccard is 1/3 = 0.333 < 0.60, which previously bypassed DUP-CLUSTER.
        self.assertAlmostEqual(
            tb.jaccard(set(orig["paths_sample"]), set(copycat["paths_sample"])),
            1.0 / 3.0,
        )

        items, clusters = tb.classify(cfg, [copycat, orig])
        by_num = {it["number"]: it for it in items}

        self.assertEqual(clusters, [[2614, 2836]])
        self.assertEqual(by_num[2614]["bucket"], "LAND-CANDIDATE")
        self.assertIsNone(by_num[2614].get("dup_of"))
        self.assertEqual(by_num[2614].get("dup_members"), [2836])

        self.assertEqual(by_num[2836]["bucket"], "DUP")
        self.assertEqual(by_num[2836].get("dup_of"), 2614)
        self.assertIn("triage:duplicate", by_num[2836]["labels"])

    def test_copycat_paraphrased_title_same_issue_and_shared_source_caught_as_dup(self):
        cfg = _cfg()
        orig = _pr(
            2614,
            "fix(hy-check): release transitive reborrows when inner block exits (#2613)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/reborrow_scope_exit_release.rs",
            ],
            additions=150,
            body="Fixes #2613",
        )
        copycat = _pr(
            2836,
            "hy-check: purge child-origin loan entries on scope exit",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/minified_reborrow.rs",
            ],
            additions=35,
            body="Resolves #2613 with a compact test.",
        )

        items, clusters = tb.classify(cfg, [copycat, orig])
        by_num = {it["number"]: it for it in items}

        self.assertEqual(clusters, [[2614, 2836]])
        self.assertEqual(by_num[2614]["bucket"], "LAND-CANDIDATE")
        self.assertEqual(by_num[2836]["bucket"], "DUP")
        self.assertEqual(by_num[2836].get("dup_of"), 2614)

    def test_unrelated_prs_touching_loans_rs_for_different_issues_stay_independent(self):
        cfg = _cfg()
        pr_a = _pr(
            2614,
            "fix(hy-check): clear child-origin reborrow loans in release_loans_originating_at (fixes #2613)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/reborrow_scope_exit_release.rs",
            ],
            additions=120,
            body="Fixes #2613",
        )
        pr_b = _pr(
            2705,
            "fix(hy-check): reject conflicting mutable field borrow across loop iteration (fixes #2704)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/loop_field_reborrow_conflict.rs",
            ],
            additions=95,
            body="Fixes #2704",
        )

        items, clusters = tb.classify(cfg, [pr_b, pr_a])
        by_num = {it["number"]: it for it in items}

        self.assertEqual(clusters, [])
        self.assertEqual(by_num[2614]["bucket"], "LAND-CANDIDATE")
        self.assertEqual(by_num[2705]["bucket"], "LAND-CANDIDATE")
        self.assertIsNone(by_num[2614].get("dup_of"))
        self.assertIsNone(by_num[2705].get("dup_of"))

    def test_pareto_board_breaks_score_ties_by_earliest_pr_number_not_min_additions(self):
        cfg = _cfg(pareto_n=5)
        early_thorough = _pr(
            2614,
            "fix(hy-check): thorough multi-case regression suite (fixes #2613)",
            [
                "compiler/crates/hy-check/src/loans.rs",
                "compiler/crates/hy-check/tests/reborrow_scope_exit_release.rs",
            ],
            additions=140,
        )
        late_golfed = _pr(
            2900,
            "fix(hy-syntax): golfed one-liner test (fixes #2899)",
            [
                "compiler/crates/hy-syntax/src/lexer.rs",
                "compiler/crates/hy-syntax/tests/golfed_lexer.rs",
            ],
            additions=15,
        )

        items, clusters = tb.classify(cfg, [late_golfed, early_thorough])
        _, pareto = tb.render_board(cfg, items, clusters, {}, "2026-11-01 00:00 UTC")

        self.assertEqual([it["number"] for it in pareto], [2614, 2900])


if __name__ == "__main__":
    unittest.main()
