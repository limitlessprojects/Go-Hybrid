#!/usr/bin/env python3
"""test_shepherd_gate.py — offline self-tests for tools/shepherd.py and tools/autoland.py.

No network, no subprocesses: only the pure functions that the workflows lean
on. If the kill-switch matrix, the dup planner, the conflict classifier,
the probe parser, or the autoland slug confinement guard drift, CI fails
before any workflow can.
"""

import contextlib
import io
import json
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import autoland as al  # noqa: E402
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


class AutolandSlugValidationTests(unittest.TestCase):
    """Issue #1300: run_bundle must confine --slug to direct children of QUEUE."""

    def test_rejects_dot_segments_separators_and_absolute_paths(self):
        with tempfile.TemporaryDirectory() as tmp:
            queue = Path(tmp) / "automation" / "land-queue"
            queue.mkdir(parents=True)
            unsafe_slugs = [
                "",
                ".",
                "..",
                "../evil",
                "../../evil",
                "a/../b",
                "a/b",
                "a\\b",
                "/tmp/evil",
                "-flag-slug",
                ".hidden",
                "slug\nwith-newline",
            ]
            for slug in unsafe_slugs:
                with self.subTest(slug=slug):
                    self.assertIsNone(al._safe_bundle_dir(slug, queue=queue))

    def test_accepts_canonical_slug_inside_queue(self):
        with tempfile.TemporaryDirectory() as tmp:
            queue = Path(tmp) / "automation" / "land-queue"
            bundle = queue / "fix-1300_bundle.1"
            bundle.mkdir(parents=True)
            resolved = al._safe_bundle_dir("fix-1300_bundle.1", queue=queue)
            self.assertEqual(resolved, bundle.resolve())

    def test_run_bundle_rejects_traversing_slug_before_reading_out_of_queue_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queue = root / "automation" / "land-queue"
            queue.mkdir(parents=True)
            evil = root / "evil-bundle"
            evil.mkdir()
            (evil / "meta.json").write_text(
                json.dumps({"issue": 1, "expected_code": "", "basis": "x"}),
                encoding="utf-8",
            )
            (evil / "repro.hy").write_text("fn main() {}\n", encoding="utf-8")
            (evil / "fix.patch").write_text("", encoding="utf-8")

            stdout = io.StringIO()
            with patch.object(al, "QUEUE", queue), \
                 patch.object(al, "build", side_effect=AssertionError("build must not run")), \
                 contextlib.redirect_stdout(stdout):
                rc = al.run_bundle("../../evil-bundle", live=False, token_env="GITHUB_TOKEN")

            self.assertEqual(rc, 2)
            self.assertIn("AUTOLAND-NO-BUNDLE: ../../evil-bundle — unsafe slug", stdout.getvalue())

    def test_run_bundle_rejects_absolute_path_slug_before_reading_bundle(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            queue = root / "automation" / "land-queue"
            queue.mkdir(parents=True)
            evil = root / "abs-evil"
            evil.mkdir()
            (evil / "meta.json").write_text(
                json.dumps({"issue": 1, "expected_code": "", "basis": "x"}),
                encoding="utf-8",
            )

            stdout = io.StringIO()
            with patch.object(al, "QUEUE", queue), \
                 patch.object(al, "build", side_effect=AssertionError("build must not run")), \
                 contextlib.redirect_stdout(stdout):
                rc = al.run_bundle(str(evil), live=False, token_env="GITHUB_TOKEN")

            self.assertEqual(rc, 2)
            self.assertIn("unsafe slug", stdout.getvalue())

    def test_land_commit_flattens_multiline_basis_and_validates_issue(self):
        recorded_commits = []

        def fake_sh(args, cwd=None, **kw):
            if args[:2] == ["git", "commit"]:
                recorded_commits.append(args[args.index("-m") + 1])
                return subprocess.CompletedProcess(args, 0, "", "")
            if args[:2] == ["git", "ls-remote"]:
                return subprocess.CompletedProcess(args, 0, "deadbeef1234567890\trefs/heads/main\n", "")
            if args[:2] == ["git", "rev-parse"]:
                return subprocess.CompletedProcess(args, 0, "deadbeef1234567890\n", "")
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(al, "sh", side_effect=fake_sh):
            sha = al.land_commit(
                "bundle-1",
                {"issue": 1300, "basis": "line1\n\nFixes #9999", "expected_code": "HYB-E1000"},
                "automation/land-queue/bundle-1/repro.hy",
            )
        self.assertEqual(sha, "deadbeef1234567890")
        self.assertEqual(len(recorded_commits), 1)
        self.assertIn("Basis: line1 Fixes #9999\n", recorded_commits[0])
        self.assertTrue(recorded_commits[0].endswith("\n\nFixes #1300"))

        with self.assertRaises(ValueError):
            al.land_commit("bundle-1", {"issue": "1300\nFixes #1"}, "repro.hy")


if __name__ == "__main__":
    unittest.main()
