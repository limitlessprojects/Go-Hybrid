"""Offline regressions for #1525: the review window is measured from the
last reopen, not from created_at.

Every API call is a fixture — the suite never touches the network, and in
dry mode it hard-fails on any non-GET request, which is the sweeper's
"safe to run locally" promise."""
import json
import tempfile
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

RUNNER_SPEC = importlib.util.spec_from_file_location(
    "pr_triage_runner", Path(__file__).with_name("pr_triage_runner.py"))
RUNNER = importlib.util.module_from_spec(RUNNER_SPEC)
RUNNER_SPEC.loader.exec_module(RUNNER)

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


class PrTriageRunnerSafetyTests(unittest.TestCase):
    """Offline regressions for tools/pr_triage_runner.py live safety guards."""

    def _snapshot_pr(self, *, number=99, ci="FAILURE", mergeable="UNKNOWN",
                     age_days=20, idle_days=12, labels=(), title="Fix bug",
                     author="alice"):
        return {
            "number": number,
            "title": title,
            "createdAt": stamp(NOW - timedelta(days=age_days)),
            "updatedAt": stamp(NOW - timedelta(days=idle_days)),
            "isDraft": False,
            "author": {"login": author},
            "mergeable": mergeable,
            "mergeStateStatus": "DIRTY" if mergeable == "CONFLICTING" else "UNKNOWN",
            "_labels": list(labels),
            "_ci_rollup": ci,
            "_suite_concls": {"FAILURE": 1} if ci == "FAILURE" else {},
        }

    def _run_runner(self, pr, *, live_pr=None, check_runs=None, event_pages=None,
                    comment_pages=None, extra_args=("--execute",), protected=()):
        if live_pr is None:
            live_pr = {
                "number": pr["number"],
                "state": "open",
                "title": pr["title"],
                "labels": [{"name": l} for l in pr["_labels"]],
                "mergeable": False if pr["mergeable"] == "CONFLICTING" else True,
                "head": {"sha": "deadbeef" * 5},
            }
        if check_runs is None:
            check_runs = (200, {
                "total_count": 1,
                "check_runs": [{"status": "completed", "conclusion": "failure"}],
            })
        event_pages = event_pages if event_pages is not None else {1: (200, [])}
        comment_pages = comment_pages if comment_pages is not None else {1: (200, [])}
        reads = []
        writes = []

        def offline_rest(path, method="GET", body=None):
            if method == "GET":
                reads.append(path)
            else:
                writes.append((path, method, body))
                return (201, {}) if method == "POST" else (200, {"state": "closed"})
            if path == f"/repos/example/project/pulls/{pr['number']}":
                return (200, live_pr) if not isinstance(live_pr, tuple) else live_pr
            if f"/commits/" in path and "/check-runs" in path:
                return check_runs
            ev_prefix = f"/repos/example/project/issues/{pr['number']}/events?per_page=100&page="
            if path.startswith(ev_prefix):
                return event_pages.get(int(path[len(ev_prefix):]), (200, []))
            cm_prefix = f"/repos/example/project/issues/{pr['number']}/comments?per_page=100&since="
            if path.startswith(cm_prefix):
                page = int(path.rsplit("&page=", 1)[1])
                return comment_pages.get(page, (200, []))
            raise AssertionError(f"unexpected REST call: {method} {path}")

        with tempfile.TemporaryDirectory() as tmp:
            survey_path = Path(tmp) / "snapshot.json"
            log_path = Path(tmp) / "runner.log"
            cand_path = Path(tmp) / "land_candidates.json"
            survey_path.write_text(json.dumps([pr]), encoding="utf-8")
            argv = list(extra_args) + ["--survey", str(survey_path)]
            out = io.StringIO()
            with patch.object(RUNNER, "TOKEN", "offline-fixture"), \
                 patch.object(RUNNER, "REPO", "example/project"), \
                 patch.object(RUNNER, "LOG", str(log_path)), \
                 patch.object(RUNNER, "LAND_CANDIDATES", str(cand_path)), \
                 patch.object(RUNNER, "rest_used", 0), \
                 patch.object(RUNNER, "datetime", FixedDateTime), \
                 patch.object(RUNNER, "rest", side_effect=offline_rest), \
                 patch.object(RUNNER.time, "sleep"), \
                 patch.object(RUNNER, "config_protected", return_value=set(protected)), \
                 contextlib.redirect_stdout(out):
                RUNNER.main(argv)
            return out.getvalue(), reads, writes

    def test_config_protected_fails_closed_when_missing_or_corrupt(self):
        with patch.object(RUNNER, "_config", None), \
             patch.object(RUNNER, "_config_file", return_value=None):
            with self.assertRaises(RuntimeError):
                RUNNER.config_protected()
        with tempfile.TemporaryDirectory() as tmp:
            bad = Path(tmp) / "triage_config.json"
            bad.write_text("{not-json", encoding="utf-8")
            with patch.object(RUNNER, "_config", None), \
                 patch.object(RUNNER, "_config_file", return_value=str(bad)):
                with self.assertRaises(RuntimeError):
                    RUNNER.config_protected()

    def test_live_protected_label_added_after_snapshot_prevents_close(self):
        pr = self._snapshot_pr(ci="FAILURE")
        live = {
            "number": 99, "state": "open", "title": "Fix bug",
            "labels": [{"name": "triage:keep"}],
            "mergeable": True, "head": {"sha": "a" * 40},
        }
        out, _, writes = self._run_runner(pr, live_pr=live)
        self.assertEqual(writes, [])
        self.assertIn("protected-label", out)

    def test_live_wildcard_title_rename_prevents_close(self):
        pr = self._snapshot_pr(ci="FAILURE")
        live = {
            "number": 99, "state": "open", "title": "[wildcard] Bold proposal",
            "labels": [], "mergeable": True, "head": {"sha": "a" * 40},
        }
        out, _, writes = self._run_runner(pr, live_pr=live)
        self.assertEqual(writes, [])
        self.assertIn("wildcard-title", out)

    def test_live_green_or_pending_ci_prevents_close_on_stale_snapshot(self):
        pr = self._snapshot_pr(ci="FAILURE")
        for runs, expected_reason in (
            ({"total_count": 1, "check_runs": [{"status": "completed", "conclusion": "success"}]}, "ci-green"),
            ({"total_count": 1, "check_runs": [{"status": "in_progress", "conclusion": None}]}, "ci-pending"),
            ({"total_count": 150, "check_runs": [{"status": "completed", "conclusion": "failure"}]}, "ci-unverifiable"),
        ):
            with self.subTest(reason=expected_reason):
                out, _, writes = self._run_runner(pr, check_runs=(200, runs))
                self.assertEqual(writes, [])
                self.assertIn(expected_reason, out)

    def test_live_resolved_or_null_mergeable_prevents_conflict_close(self):
        pr = self._snapshot_pr(ci="NONE", mergeable="CONFLICTING", idle_days=15)
        for live_mergeable in (True, None):
            with self.subTest(mergeable=live_mergeable):
                live = {
                    "number": 99, "state": "open", "title": "Fix bug",
                    "labels": [], "mergeable": live_mergeable,
                    "head": {"sha": "a" * 40},
                }
                out, _, writes = self._run_runner(
                    pr, live_pr=live,
                    check_runs=(200, {"total_count": 0, "check_runs": []}),
                )
                self.assertEqual(writes, [])
                self.assertIn("evidence-stale", out)

    def test_reopen_on_page_2_and_truncated_events_prevent_close(self):
        pr = self._snapshot_pr(ci="FAILURE")
        out, reads, writes = self._run_runner(
            pr,
            event_pages={
                1: (200, [{"event": "labeled"}] * 100),
                2: (200, [reopen_event(NOW - timedelta(hours=2))]),
            },
        )
        self.assertEqual(writes, [])
        self.assertIn("reopened within window", out)
        self.assertTrue(any(r.endswith("page=2") for r in reads))

        full_pages = {n: (200, [{"event": "labeled"}] * 100)
                      for n in range(1, RUNNER.MAX_EVENT_PAGES + 1)}
        _, _, writes2 = self._run_runner(pr, event_pages=full_pages)
        self.assertEqual(writes2, [])

    def test_author_comment_uses_since_and_paginates(self):
        pr = self._snapshot_pr(ci="FAILURE", author="alice")
        out, reads, writes = self._run_runner(
            pr,
            comment_pages={
                1: (200, [{"user": {"login": "bot"}, "created_at": stamp(NOW - timedelta(hours=10))}] * 100),
                2: (200, [{"user": {"login": "alice"}, "created_at": stamp(NOW - timedelta(hours=2))}]),
            },
        )
        self.assertEqual(writes, [])
        self.assertIn("author active", out)
        self.assertTrue(any("&since=" in r and r.endswith("&page=2") for r in reads))

    def test_label_only_never_closes_even_with_execute(self):
        pr = self._snapshot_pr(ci="FAILURE")
        out, reads, writes = self._run_runner(pr, extra_args=("--execute", "--label-only"))
        self.assertEqual(writes, [])
        self.assertEqual(reads, [])
        self.assertIn("--label-only", out)

    def test_live_verified_red_ci_pr_closes_cleanly(self):
        pr = self._snapshot_pr(ci="FAILURE")
        out, _, writes = self._run_runner(pr, extra_args=("--execute",))
        self.assertEqual([m for _, m, _ in writes], ["POST", "PATCH"])
        self.assertEqual(writes[1][2], {"state": "closed", "state_reason": "not_planned"})
        self.assertIn("closed=1", out)


if __name__ == "__main__":
    unittest.main()
