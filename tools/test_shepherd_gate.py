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

    def test_closed_member_is_filtered(self):
        # Reproducer for #3031: closed cluster members must not be planned for Phase A
        items = [_item(5, dup_members=[6, 7]), _item(6), _item(7)]
        clusters = [[5, 6, 7]]
        plan = sh.plan_phase_a(items, clusters, is_open=lambda n: n in (5, 7))
        self.assertEqual(plan, [(5, [7])])


class LabelAgeTests(unittest.TestCase):
    """Phase A / B dup handling tracks label age and respects unlabel events (#3031)."""

    def test_unlabeled_resets_label_age(self):
        gh = unittest.mock.MagicMock()
        events = [
            {"event": "labeled", "label": {"name": "bot:dup-of:#1"}, "created_at": "2026-10-01T00:00:00Z"},
            {"event": "unlabeled", "label": {"name": "bot:dup-of:#1"}, "created_at": "2026-10-02T00:00:00Z"},
        ]
        gh.req.return_value = (200, events)
        from datetime import datetime, timezone
        now = datetime(2026, 10, 5, 0, 0, tzinfo=timezone.utc)
        age = sh._label_age_days(gh, 10, "bot:dup-of:#1", now)
        self.assertIsNone(age)

    def test_relabeled_uses_newest_labeled_date(self):
        gh = unittest.mock.MagicMock()
        events = [
            {"event": "labeled", "label": {"name": "bot:dup-of:#1"}, "created_at": "2026-10-01T00:00:00Z"},
            {"event": "unlabeled", "label": {"name": "bot:dup-of:#1"}, "created_at": "2026-10-02T00:00:00Z"},
            {"event": "labeled", "label": {"name": "bot:dup-of:#1"}, "created_at": "2026-10-04T00:00:00Z"},
        ]
        gh.req.return_value = (200, events)
        from datetime import datetime, timezone
        now = datetime(2026, 10, 5, 0, 0, tzinfo=timezone.utc)
        age = sh._label_age_days(gh, 10, "bot:dup-of:#1", now)
        self.assertEqual(age, 1.0)


class MergeMsgParsing(unittest.TestCase):
    """W5 merge messages must extract all issue refs including Closes/Resolves (#3031)."""

    def test_parse_issue_refs_single_fix(self):
        self.assertEqual(sh.parse_issue_refs("Fixes #123"), ["123"])

    def test_parse_issue_refs_closes_resolves_and_colons(self):
        text = "Closes #821\nResolves: #867\nFixes: #900"
        self.assertEqual(sh.parse_issue_refs(text), ["821", "867", "900"])

    def test_parse_issue_refs_deduplicates(self):
        text = "fix #100, Fixes: #100\n(fixes #100)"
        self.assertEqual(sh.parse_issue_refs(text), ["100"])

    def test_parse_issue_refs_empty(self):
        self.assertEqual(sh.parse_issue_refs("no issue refs here"), [])


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


class WorkflowWorkspacePaths(unittest.TestCase):
    """Shepherd workflows run from repo root, while Cargo workspaces live under
    compiler/ and runtime/rust/hyrt/ and Go runtime packages live under
    runtime/go (assembled as module hybrid.program, no root go.mod) (#2487)."""

    ROOT = Path(__file__).resolve().parent.parent

    def test_shepherd_setup_caches_actual_workspaces(self):
        src = (self.ROOT / ".github" / "actions" / "shepherd-setup" / "action.yml").read_text(encoding="utf-8")
        self.assertIn("compiler/target", src)
        self.assertIn("runtime/rust/hyrt/target", src)
        self.assertIn("compiler/Cargo.lock", src)
        self.assertIn("runtime/rust/hyrt/Cargo.lock", src)

    def test_shepherd_preflight_targets_compiler_and_synthesized_go_module(self):
        src = (self.ROOT / ".github" / "workflows" / "shepherd-preflight.yml").read_text(encoding="utf-8")
        self.assertIn("--manifest-path compiler/Cargo.toml -p hyb", src)
        self.assertIn("--manifest-path runtime/rust/hyrt/Cargo.toml", src)
        self.assertIn("module hybrid.program", src)
        self.assertNotIn("if go build ./... ; then", src)
        self.assertNotIn("if cargo build -p hyb --quiet ; then", src)

    def test_shepherd_verify_passes_manifest_and_hyb_bin(self):
        src = (self.ROOT / ".github" / "workflows" / "shepherd-verify.yml").read_text(encoding="utf-8")
        self.assertIn("cargo test --manifest-path compiler/Cargo.toml --workspace", src)
        self.assertIn("cargo build --manifest-path compiler/Cargo.toml -p hyb", src)
        self.assertIn("tools/corpus_harness.py --bin compiler/target/debug/hyb", src)
        self.assertIn("tools/differential_loop.py --bin compiler/target/debug/hyb", src)


class CmdProbeTests(unittest.TestCase):
    """cmd_probe must not crash with NameError on unmatched diffs or TypeError
    subscripting subprocess.CompletedProcess after a matched probe (#2809)."""

    def test_unmatched_diff_prints_no_premise_patterns(self):
        import io, tempfile
        from contextlib import redirect_stdout
        from types import SimpleNamespace
        with tempfile.NamedTemporaryFile("w", suffix=".diff", delete=False) as tf:
            tf.write("diff --git a/x.hy b/x.hy\n+++ b/x.hy\n+let a = 42\n")
            p = tf.name
        try:
            out = io.StringIO()
            with redirect_stdout(out):
                sh.cmd_probe(SimpleNamespace(diff_file=p, go="go"))
            self.assertIn("no premise patterns matched in diff", out.getvalue())
        finally:
            Path(p).unlink(missing_ok=True)

    def test_matched_probe_rejected_does_not_subscript_completed_process(self):
        import io, subprocess, tempfile
        from contextlib import redirect_stdout
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.NamedTemporaryFile("w", suffix=".diff", delete=False) as tf:
            tf.write("+let c = make(<-chan int, 1)\n")
            p = tf.name
        try:
            fake = subprocess.CompletedProcess(["go"], 1, "", "cannot make <-chan int\n")
            out = io.StringIO()
            with patch.object(sh.subprocess, "run", return_value=fake), redirect_stdout(out):
                sh.cmd_probe(SimpleNamespace(diff_file=p, go="go"))
            self.assertIn("probe-ok: make-directional-buffered", out.getvalue())
            self.assertNotIn("no premise patterns matched in diff", out.getvalue())
        finally:
            Path(p).unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
