"""Offline regressions for the PR queue sweeper and snapshot tools.

Every API call is a fixture — the suite never touches the network, and in
dry mode it hard-fails on any non-GET request, which is the sweeper's
"safe to run locally" promise."""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
import os
from pathlib import Path
import runpy
import sys
import tempfile
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


class QueueSnapshotTests(unittest.TestCase):
    def test_output_path_with_or_without_parent_directory(self):
        script = Path(__file__).resolve().with_name("queue_snapshot.py")
        pull_requests = {"nodes": [], "pageInfo": {"hasNextPage": False}}
        response = json.dumps({"data": {"repository": {"pullRequests": pull_requests}}}).encode()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outputs = ("snapshot.json", "nested/snapshot.json", str(root / "absolute" / "snapshot.json"))
            with contextlib.chdir(root):
                for output in outputs:
                    with (
                        self.subTest(output=output),
                        patch.dict(os.environ, {"GH_TOKEN": "test-token", "SURVEY_OUT": output}),
                        patch("urllib.request.urlopen", return_value=io.BytesIO(response)),
                        contextlib.redirect_stdout(io.StringIO()),
                    ):
                        runpy.run_path(str(script))
                        self.assertEqual([], json.loads(Path(output).read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
