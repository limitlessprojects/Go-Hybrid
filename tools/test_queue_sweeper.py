"""Offline regressions for #1525: the review window is measured from the
last reopen, not from created_at.

Every API call is a fixture — the suite never touches the network, and in
dry mode it hard-fails on any non-GET request, which is the sweeper's
"safe to run locally" promise."""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "queue_sweeper", Path(__file__).with_name("queue_sweeper.py"))
SWEEPER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SWEEPER)

NOW = datetime(2026, 10, 24, 15, tzinfo=timezone.utc)
CUTOFF = NOW - timedelta(days=SWEEPER.MIN_AGE)


def stamp(value):
    return value.isoformat().replace("+00:00", "Z")


class FixedDateTime(datetime):
    """Patched over the module-global `datetime` so main()'s clock is frozen."""
    @classmethod
    def now(cls, tz=None):
        return NOW if tz is not None else NOW.replace(tzinfo=None)


def reopen_event(at):
    return {"event": "reopened", "created_at": stamp(at)}


class ReviewWindowPolicyTests(unittest.TestCase):
    def run_sweep(self, *, created_at=stamp(NOW - timedelta(days=20)),
                  updated_at=stamp(NOW - timedelta(hours=1)), pages=None,
                  labels=(), protected=(), dry=True, extra_args=()):
        pr = {"number": 42, "created_at": created_at, "updated_at": updated_at,
              "labels": [{"name": name} for name in labels],
              "title": "Offline contribution", "user": {"login": "fixture"}}
        pages = pages or {}
        reads = []
        self.writes = []

        def offline_api(path, method="GET", body=None):
            if method == "GET":
                reads.append(path)
            else:
                self.assertFalse(dry, f"dry run must not write: {method} {path}")
                self.writes.append((path, method, body))
                return (200, {"state": "closed"}) if method == "PATCH" else (201, {})
            if path == "/repos/example/project":
                return 200, {"open_issues_count": 0}
            if path.startswith("/repos/example/project/pulls?"):
                return 200, [pr]
            prefix = "/repos/example/project/issues/42/events?per_page=100&page="
            if path.startswith(prefix):
                return pages.get(int(path[len(prefix):]), (200, []))
            raise AssertionError(f"unexpected API path: {path}")

        argv = ["queue_sweeper"] + (["--dry"] if dry else []) + list(extra_args)
        out = io.StringIO()
        with patch.object(SWEEPER, "TOK", "offline-fixture"), \
             patch.object(SWEEPER, "REPO", "example/project"), \
             patch.object(SWEEPER, "datetime", FixedDateTime), \
             patch.object(SWEEPER, "api", side_effect=offline_api), \
             patch.object(SWEEPER.time, "sleep"), \
             patch.object(SWEEPER, "config_protected_numbers", return_value=set(protected)), \
             patch.object(sys, "argv", argv), \
             contextlib.redirect_stdout(out):
            SWEEPER.main()
        return out.getvalue(), reads

    def assert_due(self, output, count):
        self.assertIn(f"due for close: {count}\n", output)

    # -- reopen inside the window protects ------------------------------------

    def test_reopen_inside_window_is_not_due(self):
        output, _ = self.run_sweep(pages={1: (200, [reopen_event(NOW - timedelta(hours=1))])})
        self.assert_due(output, 0)

    def test_reopen_on_a_later_event_page_is_not_due(self):
        output, reads = self.run_sweep(pages={
            1: (200, [{"event": "labeled"}] * 100),
            2: (200, [reopen_event(NOW)]),
        })
        self.assert_due(output, 0)
        self.assertTrue(any(p.endswith("page=2") for p in reads))

    def test_reopen_exactly_at_cutoff_is_still_inside_the_window(self):
        output, _ = self.run_sweep(pages={1: (200, [reopen_event(CUTOFF)])})
        self.assert_due(output, 0)

    # -- activity that is NOT a reopen does not renew --------------------------

    def test_recent_unrelated_activity_does_not_protect(self):
        output, _ = self.run_sweep(pages={1: (200, [{"event": "labeled"}])})
        self.assert_due(output, 1)

    def test_reopen_older_than_the_window_does_not_protect(self):
        output, _ = self.run_sweep(pages={1: (200, [reopen_event(CUTOFF - timedelta(seconds=1))])})
        self.assert_due(output, 1)

    # -- unreadable history fails open (never close on a guess) ----------------

    def test_events_lookup_failure_keeps_the_pr(self):
        output, _ = self.run_sweep(pages={1: (403, None)})
        self.assert_due(output, 0)
        self.assertIn("reopen history unavailable", output)

    def test_malformed_history_keeps_the_pr(self):
        for events in ([None], [{"event": 42}],
                       [{"event": "reopened", "created_at": "not-a-time"}]):
            with self.subTest(events=events):
                output, _ = self.run_sweep(pages={1: (200, events)})
                self.assert_due(output, 0)

    def test_history_beyond_the_page_cap_keeps_the_pr(self):
        full = {n: (200, [{"event": "labeled"}] * 100)
                for n in range(1, SWEEPER.MAX_EVENT_PAGES + 1)}
        output, reads = self.run_sweep(pages=full)
        self.assert_due(output, 0)
        self.assertTrue(any(p.endswith(f"page={SWEEPER.MAX_EVENT_PAGES}") for p in reads))

    def test_unparseable_created_at_is_never_closed(self):
        for created in (None, "", "garbage"):
            with self.subTest(created_at=created):
                output, _ = self.run_sweep(created_at=created)
                self.assert_due(output, 0)

    # -- event lookups are spent only when they can matter ---------------------

    def test_old_updated_at_skips_the_events_lookup(self):
        output, reads = self.run_sweep(updated_at=stamp(CUTOFF - timedelta(seconds=1)))
        self.assert_due(output, 1)
        self.assertEqual([p for p in reads if "/events" in p], [])

    def test_fresh_arrival_skips_the_events_lookup(self):
        output, reads = self.run_sweep(created_at=stamp(CUTOFF))
        self.assert_due(output, 0)
        self.assertEqual([p for p in reads if "/events" in p], [])

    def test_missing_updated_at_still_consults_events(self):
        output, reads = self.run_sweep(updated_at=None, pages={1: (200, [reopen_event(NOW)])})
        self.assert_due(output, 0)
        self.assertTrue(any("/events" in p for p in reads))

    def test_existing_protections_short_circuit_before_events(self):
        for kwargs in ({"labels": ["triage:keep"]}, {"protected": [42]}):
            with self.subTest(**kwargs):
                output, reads = self.run_sweep(**kwargs)
                self.assert_due(output, 0)
                self.assertEqual([p for p in reads if "/events" in p], [])

    def test_full_first_page_forces_the_next_page_before_aging(self):
        output, reads = self.run_sweep(pages={1: (200, [{"event": "labeled"}] * 100)})
        self.assert_due(output, 1)
        self.assertTrue(any(p.endswith("page=2") for p in reads))

    # -- the written close reply cites the configured window, not the PR age ---

    def test_close_comment_shows_the_review_window_not_the_age(self):
        for window in ("3", "5"):
            with self.subTest(min_age=window):
                output, _ = self.run_sweep(dry=False,
                                           extra_args=("--min-age", window))
                self.assert_due(output, 1)
                self.assertIn("DONE: closed=1 fails=0", output)
                self.assertEqual([m for _, m, _ in self.writes], ["PATCH", "POST"])
                comment = self.writes[1][2]["body"]
                self.assertIn(f"review window ({window} days)", comment)
                self.assertNotIn("20 days", comment)


SNAPSHOT_SPEC = importlib.util.spec_from_file_location(
    "queue_snapshot", Path(__file__).with_name("queue_snapshot.py"))
SNAPSHOT = importlib.util.module_from_spec(SNAPSHOT_SPEC)
SNAPSHOT_SPEC.loader.exec_module(SNAPSHOT)

RUNNER_SPEC = importlib.util.spec_from_file_location(
    "pr_triage_runner", Path(__file__).with_name("pr_triage_runner.py"))
RUNNER = importlib.util.module_from_spec(RUNNER_SPEC)
RUNNER_SPEC.loader.exec_module(RUNNER)


class TriageRunnerNonGateCheckTests(unittest.TestCase):
    """Regressions for non-gate housekeeping checks (e.g. `board`) poisoning
    GitHub's statusCheckRollup on otherwise green fork PRs."""

    @staticmethod
    def make_pr(number=2851, *, raw_state="FAILURE", contexts=None,
                mergeable="MERGEABLE", merge_state="UNSTABLE", age_days=18.0):
        pr = {
            "number": number,
            "title": "fix(hy-check): regression fixture",
            "_labels": [],
            "_age_days": age_days,
            "_idle_days": age_days,
            "_ci_rollup": raw_state,
            "_suite_concls": {},
            "mergeable": mergeable,
            "mergeStateStatus": merge_state,
        }
        if contexts is not None:
            pr["commits"] = {
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": {
                            "state": raw_state,
                            "contexts": {"nodes": contexts},
                        }
                    }
                }]
            }
        return pr

    def test_snapshot_ignores_board_failure_when_gates_are_green(self):
        rollup = {
            "state": "FAILURE",
            "contexts": {
                "nodes": [
                    {"__typename": "CheckRun", "name": "compiler", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "quick", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "preflight", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "board", "status": "COMPLETED", "conclusion": "FAILURE"},
                ]
            },
        }
        state, failed, ignored = SNAPSHOT.summarize_ci(rollup)
        self.assertEqual(state, "SUCCESS")
        self.assertEqual(failed, {})
        self.assertEqual(ignored, {"board": "FAILURE"})

    def test_snapshot_ignores_board_cancelled_when_gates_are_green(self):
        rollup = {
            "state": "FAILURE",
            "contexts": {
                "nodes": [
                    {"__typename": "CheckRun", "name": "compiler", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "quick", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "board", "status": "COMPLETED", "conclusion": "CANCELLED"},
                ]
            },
        }
        state, failed, ignored = SNAPSHOT.summarize_ci(rollup)
        self.assertEqual(state, "SUCCESS")
        self.assertEqual(failed, {})
        self.assertEqual(ignored, {"board": "CANCELLED"})

    def test_snapshot_reports_real_gate_failure_with_check_name(self):
        rollup = {
            "state": "FAILURE",
            "contexts": {
                "nodes": [
                    {"__typename": "CheckRun", "name": "compiler", "status": "COMPLETED", "conclusion": "FAILURE"},
                    {"__typename": "CheckRun", "name": "quick", "status": "COMPLETED", "conclusion": "SUCCESS"},
                    {"__typename": "CheckRun", "name": "board", "status": "COMPLETED", "conclusion": "SUCCESS"},
                ]
            },
        }
        state, failed, ignored = SNAPSHOT.summarize_ci(rollup)
        self.assertEqual(state, "FAILURE")
        self.assertEqual(failed, {"compiler": "FAILURE"})
        self.assertEqual(ignored, {"board": "SUCCESS"})

    def test_runner_skips_and_surfaces_land_candidate_when_only_board_failed(self):
        pr = self.make_pr(
            raw_state="FAILURE",
            merge_state="UNSTABLE",
            contexts=[
                {"__typename": "CheckRun", "name": "compiler", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "name": "quick", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "name": "preflight", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "name": "board", "status": "COMPLETED", "conclusion": "FAILURE"},
            ],
        )
        with patch.object(RUNNER, "config_protected", return_value=set()):
            action, evidence, extra = RUNNER.decide(pr)
            self.assertEqual((action, evidence, extra), ("skip", [], ""))
            self.assertTrue(RUNNER.is_land_candidate(pr))

    def test_runner_closes_and_names_failing_gate_when_compiler_failed(self):
        pr = self.make_pr(
            raw_state="FAILURE",
            contexts=[
                {"__typename": "CheckRun", "name": "compiler", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"__typename": "CheckRun", "name": "quick", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "CheckRun", "name": "board", "status": "COMPLETED", "conclusion": "FAILURE"},
            ],
        )
        with patch.object(RUNNER, "config_protected", return_value=set()):
            action, evidence, extra = RUNNER.decide(pr)
            self.assertEqual((action, evidence, extra), ("close", ["ci-failure"], ""))
            self.assertFalse(RUNNER.is_land_candidate(pr))
            reply = RUNNER.reply_for(pr, evidence)
            self.assertIn("compiler=FAILURE", reply)
            self.assertNotIn("board=", reply)

    def test_live_gate_check_failure_protects_board_only_failure_on_legacy_snapshot(self):
        def fake_rest(path, method="GET", body=None):
            if path == "/repos/example/project/pulls/2851":
                return 200, {"head": {"sha": "abc123"}}
            if path == "/repos/example/project/commits/abc123/check-runs?per_page=100":
                return 200, {
                    "check_runs": [
                        {"name": "compiler", "status": "completed", "conclusion": "success"},
                        {"name": "quick", "status": "completed", "conclusion": "success"},
                        {"name": "preflight", "status": "completed", "conclusion": "success"},
                        {"name": "board", "status": "completed", "conclusion": "failure"},
                    ]
                }
            raise AssertionError(f"unexpected path: {path}")

        with patch.object(RUNNER, "REPO", "example/project"), \
             patch.object(RUNNER, "rest", side_effect=fake_rest):
            failed, details = RUNNER.live_gate_check_failure(2851)
        self.assertFalse(failed)
        self.assertEqual(details, {})

    def test_live_gate_check_failure_fails_open_on_http_error(self):
        with patch.object(RUNNER, "REPO", "example/project"), \
             patch.object(RUNNER, "rest", return_value=(503, None)):
            failed, details = RUNNER.live_gate_check_failure(2851)
        self.assertFalse(failed)
        self.assertEqual(details, {})


if __name__ == "__main__":
    unittest.main()
