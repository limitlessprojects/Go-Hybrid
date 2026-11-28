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


class StateLabelReconciliationTests(unittest.TestCase):
    """#986: sync-labels must diff against current_labels and remove stale
    state labels (`bot:land-candidate`, `bot:conflict`, `bot:stale-base`), and
    W2 rebase must clear a stale `bot:conflict` when live `mergeable` is True."""

    def test_plan_sync_labels_noop_when_already_current(self):
        it = _item(10, bucket="LAND-CANDIDATE",
                   current_labels=["bot:land-candidate", "triage:pareto"])
        self.assertEqual(sh.plan_sync_labels(it), ([], []))

    def test_plan_sync_labels_replaces_stale_conflict_and_stale_base(self):
        it = _item(11, bucket="LAND-CANDIDATE",
                   current_labels=["bot:conflict", "bot:stale-base", "triage:bug"])
        self.assertEqual(
            sh.plan_sync_labels(it),
            (["bot:land-candidate"], ["bot:conflict", "bot:stale-base"]),
        )

    def test_plan_sync_labels_clears_state_labels_on_unmapped_bucket(self):
        it = _item(12, bucket="RECON", current_labels=["bot:conflict"])
        self.assertEqual(sh.plan_sync_labels(it), ([], ["bot:conflict"]))

    def test_plan_sync_labels_skips_wildcard_and_draft(self):
        w = _item(13, bucket="CONFLICTS", wildcard_hold=True,
                  current_labels=["bot:land-candidate"])
        d = _item(14, bucket="CONFLICTS", draft_parked=True,
                  current_labels=["bot:land-candidate"])
        self.assertEqual(sh.plan_sync_labels(w), ([], []))
        self.assertEqual(sh.plan_sync_labels(d), ([], []))

    def test_cmd_sync_labels_posts_only_delta_and_deletes_stale(self):
        import json, tempfile
        from types import SimpleNamespace
        from unittest.mock import patch

        scan_data = {
            "items": [
                _item(101, bucket="LAND-CANDIDATE",
                      current_labels=["bot:land-candidate"]),
                _item(102, bucket="LAND-CANDIDATE",
                      current_labels=["bot:conflict"]),
                _item(103, bucket="RECON",
                      current_labels=["bot:stale-base"]),
            ]
        }
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as tf:
            json.dump(scan_data, tf)
            scan_path = tf.name

        posts, reqs = [], []

        class FakeGh:
            repo = "limitlessprojects/Go-Hybrid"
            calls = 0
            remaining = 4900

            def __init__(self, repo, token):
                pass

            def post(self, path, payload):
                self.calls += 1
                posts.append((path, payload))
                return {}

            def req(self, method, path, payload=None):
                self.calls += 1
                reqs.append((method, path, payload))
                return 200, {}

        try:
            with patch.object(sh.tb, "Gh", FakeGh), \
                 patch.object(sh.tb, "load_token", return_value="tok"), \
                 patch.object(sh, "ensure_palette"):
                sh.cmd_sync_labels(SimpleNamespace(from_scan=scan_path, dry=False))

            # #101 is untouched; #102 gets +bot:land-candidate and -bot:conflict;
            # #103 gets -bot:stale-base.
            self.assertEqual(
                posts,
                [("/repos/limitlessprojects/Go-Hybrid/issues/102/labels",
                  {"labels": ["bot:land-candidate"]})],
            )
            self.assertEqual(
                reqs,
                [
                    ("DELETE",
                     "/repos/limitlessprojects/Go-Hybrid/issues/102/labels/bot%3Aconflict",
                     None),
                    ("DELETE",
                     "/repos/limitlessprojects/Go-Hybrid/issues/103/labels/bot%3Astale-base",
                     None),
                ],
            )
        finally:
            Path(scan_path).unlink(missing_ok=True)

    def test_cmd_rebase_clears_stale_bot_conflict_when_mergeable_true(self):
        from types import SimpleNamespace
        from unittest.mock import patch

        reqs = []

        class FakeGh:
            repo = "limitlessprojects/Go-Hybrid"
            calls = 0
            remaining = 4900

            def __init__(self, repo, token):
                pass

            def get(self, path):
                if path.endswith("/commits/main"):
                    return {"sha": "mainsha123456"}
                if path.endswith("/pulls/711"):
                    return {"mergeable": True, "head": {"sha": "head711abcdef"}}
                if "/compare/" in path:
                    return {"behind_by": 60}
                return {}

            def paginate(self, path):
                return [{
                    "number": 711,
                    "draft": False,
                    "created_at": "2026-10-20T00:00:00Z",
                    "labels": [{"name": "bot:conflict"}],
                }]

            def req(self, method, path, payload=None):
                reqs.append((method, path, payload))
                return 202, {}

        with patch.object(sh.tb, "Gh", FakeGh), \
             patch.object(sh.tb, "load_token", return_value="tok"):
            sh.cmd_rebase(SimpleNamespace(dry=False))

        self.assertIn(
            ("DELETE",
             "/repos/limitlessprojects/Go-Hybrid/issues/711/labels/bot%3Aconflict",
             None),
            reqs,
        )
        self.assertIn(
            ("PUT",
             "/repos/limitlessprojects/Go-Hybrid/pulls/711/update-branch",
             {"expected_head_sha": "head711abcdef"}),
            reqs,
        )


if __name__ == "__main__":
    unittest.main()
