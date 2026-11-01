#!/usr/bin/env python3
import argparse
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import triage_bot


class FakeGh:
    def __init__(self, main_sha, prs, full_prs, files_by_pr, compares):
        self.repo = "limitlessprojects/Go-Hybrid"
        self.main_sha = main_sha
        self.prs = prs
        self.full_prs = full_prs
        self.files_by_pr = files_by_pr
        self.compares = compares
        self.get_calls = []

    def paginate(self, path):
        if path.startswith(f"/repos/{self.repo}/pulls?"):
            return self.prs
        for n, files in self.files_by_pr.items():
            if path == f"/repos/{self.repo}/pulls/{n}/files?per_page=100":
                return files
        return []

    def get(self, path):
        self.get_calls.append(path)
        if path == f"/repos/{self.repo}/commits/main":
            return {"sha": self.main_sha}
        for n, full in self.full_prs.items():
            if path == f"/repos/{self.repo}/pulls/{n}":
                return full
        return self.compares.get(path)


class TriageBotScanQuotaTest(unittest.TestCase):
    def test_skips_up_to_date_base_and_caches_shared_merge_base(self):
        main_sha = "aaaa111122223333"
        shared_mb = "bbbb000011112222"
        prs = [
            {
                "number": 101,
                "title": "PR already based on main",
                "user": {"login": "alice"},
                "created_at": "2026-10-25T00:00:00Z",
                "updated_at": "2026-10-25T01:00:00Z",
                "draft": False,
                "base": {"sha": main_sha},
                "head": {"sha": "1010head1010head"},
            },
            {
                "number": 102,
                "title": "First PR behind main on shared merge base",
                "user": {"login": "bob"},
                "created_at": "2026-10-25T00:00:00Z",
                "updated_at": "2026-10-25T01:00:00Z",
                "draft": False,
                "base": {"sha": shared_mb},
                "head": {"sha": "1020head1020head"},
            },
            {
                "number": 103,
                "title": "Second PR behind main on same shared merge base",
                "user": {"login": "carol"},
                "created_at": "2026-10-25T00:00:00Z",
                "updated_at": "2026-10-25T01:00:00Z",
                "draft": False,
                "base": {"sha": shared_mb},
                "head": {"sha": "1030head1030head"},
            },
        ]
        full_prs = {
            n: {
                "mergeable": True,
                "mergeable_state": "clean",
                "base": p["base"],
            }
            for p in prs
            for n in [p["number"]]
        }
        files_by_pr = {
            101: [{"filename": "compiler/crates/hy-check/src/a.rs", "additions": 5, "deletions": 1}],
            102: [{"filename": "compiler/crates/hy-check/src/shared.rs", "additions": 8, "deletions": 2}],
            103: [{"filename": "compiler/crates/hy-check/src/shared.rs", "additions": 4, "deletions": 0}],
        }
        compares = {
            f"/repos/limitlessprojects/Go-Hybrid/compare/{main_sha}...1020head1020head": {
                "behind_by": 4,
                "merge_base_commit": {"sha": shared_mb},
            },
            f"/repos/limitlessprojects/Go-Hybrid/compare/{main_sha}...1030head1030head": {
                "behind_by": 4,
                "merge_base_commit": {"sha": shared_mb},
            },
            f"/repos/limitlessprojects/Go-Hybrid/compare/{shared_mb}...{main_sha}?per_page=250": {
                "files": [{"filename": "compiler/crates/hy-check/src/shared.rs"}],
            },
        }
        gh = FakeGh(main_sha, prs, full_prs, files_by_pr, compares)
        args = argparse.Namespace(jobs=2)
        items, main_out = triage_bot.collect({}, gh, args)
        self.assertEqual(main_out, main_sha)
        by_num = {it["number"]: it for it in items}

        # PR 101 is based on main_sha -> 0 compare calls made for it.
        self.assertEqual(by_num[101]["behind_by"], 0)
        self.assertEqual(by_num[101]["overlap_risk"], [])
        self.assertNotIn(
            f"/repos/limitlessprojects/Go-Hybrid/compare/{main_sha}...1010head1010head",
            gh.get_calls,
        )

        # PRs 102 and 103 share merge_base -> cmp2 compare call happens exactly once.
        self.assertEqual(by_num[102]["overlap_risk"], ["compiler/crates/hy-check/src/shared.rs"])
        self.assertEqual(by_num[103]["overlap_risk"], ["compiler/crates/hy-check/src/shared.rs"])
        cmp2_path = f"/repos/limitlessprojects/Go-Hybrid/compare/{shared_mb}...{main_sha}?per_page=250"
        self.assertEqual(gh.get_calls.count(cmp2_path), 1)


if __name__ == "__main__":
    unittest.main()
