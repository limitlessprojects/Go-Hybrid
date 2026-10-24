"""Offline CLI policy regressions; every API call is replaced by a fixture."""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "queue_sweeper", Path(__file__).with_name("queue_sweeper.py")
)
SWEEPER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SWEEPER)
NOW = datetime(2026, 10, 24, 15, tzinfo=timezone.utc)
CUTOFF = NOW - timedelta(days=5)


def stamp(value):
    return value.isoformat().replace("+00:00", "Z")


class FixedDateTime(datetime):
    @classmethod
    def now(cls, tz=None):
        return NOW if tz is not None else NOW.replace(tzinfo=None)


class ReviewWindowTests(unittest.TestCase):
    def run_sweep(self, *, pages=None, updated_at=stamp(NOW),
                  created_at=stamp(NOW - timedelta(days=20)), labels=(),
                  protected=(), dry=True, extra_args=()):
        pr = {
            "number": 42, "created_at": created_at, "updated_at": updated_at,
            "labels": [{"name": label} for label in labels],
            "title": "Offline contribution", "user": {"login": "fixture"},
        }
        calls = []
        self.writes = []
        pages = pages or {}

        def offline_api(path, method="GET", body=None):
            calls.append(path)
            if method != "GET":
                self.assertFalse(dry, "dry preview must not write")
                self.writes.append((path, method, body))
                if method == "PATCH":
                    return 200, {"state": "closed"}
                self.assertEqual(method, "POST")
                return 201, {}
            if path == "/repos/example/project":
                return 200, {"open_issues_count": 0}
            if path == "/repos/example/project/pulls?state=open&per_page=100&page=1":
                return 200, [pr]
            prefix = "/repos/example/project/issues/42/events?per_page=100&page="
            self.assertTrue(path.startswith(prefix), path)
            return pages.get(int(path[len(prefix):]), (200, []))

        output = io.StringIO()
        with patch.object(SWEEPER, "TOK", "offline-fixture"), \
             patch.object(SWEEPER, "REPO", "example/project"), \
             patch.object(SWEEPER, "datetime", FixedDateTime), \
             patch.object(SWEEPER, "api", side_effect=offline_api), \
             patch.object(SWEEPER.time, "sleep"), \
             patch.object(SWEEPER, "config_protected_numbers", return_value=set(protected)), \
             patch.object(sys, "argv", ["queue_sweeper"] + (["--dry"] if dry else [])
                          + list(extra_args)), \
             contextlib.redirect_stdout(output):
            SWEEPER.main()
        return output.getvalue(), calls

    def assert_due(self, output, count):
        self.assertIn(f"due for close: {count}\n", output)

    def test_recent_reopen_receives_a_new_review_window(self):
        output, _ = self.run_sweep(pages={1: (200, [
            {"event": "reopened", "created_at": stamp(NOW - timedelta(hours=1))},
        ])})
        self.assert_due(output, 0)

    def test_reopen_on_second_event_page_receives_a_new_window(self):
        output, calls = self.run_sweep(pages={
            1: (200, [{"event": "labeled"}] * 100),
            2: (200, [{"event": "reopened", "created_at": stamp(NOW)}]),
        })
        self.assert_due(output, 0)
        self.assertTrue(any(path.endswith("page=2") for path in calls))

    def test_unrelated_recent_activity_does_not_restart_the_window(self):
        output, _ = self.run_sweep(pages={1: (200, [{"event": "labeled"}])})
        self.assert_due(output, 1)

    def test_old_reopen_does_not_protect_an_expired_window(self):
        output, _ = self.run_sweep(pages={1: (200, [
            {"event": "reopened", "created_at": stamp(CUTOFF - timedelta(seconds=1))},
        ])})
        self.assert_due(output, 1)

    def test_cutoff_equality_is_still_inside_the_window(self):
        output, _ = self.run_sweep(pages={1: (200, [
            {"event": "reopened", "created_at": stamp(CUTOFF)},
        ])})
        self.assert_due(output, 0)

    def test_old_update_avoids_unneeded_event_requests(self):
        output, calls = self.run_sweep(updated_at=stamp(CUTOFF - timedelta(seconds=1)))
        self.assert_due(output, 1)
        self.assertEqual(len(calls), 1)

    def test_fresh_original_arrival_needs_no_event_requests(self):
        output, calls = self.run_sweep(created_at=stamp(CUTOFF))
        self.assert_due(output, 0)
        self.assertEqual(len(calls), 1)

    def test_existing_label_and_config_protection_remain_effective(self):
        for kwargs in ({"labels": ["triage:keep"]}, {"protected": [42]}):
            with self.subTest(kwargs=kwargs):
                output, calls = self.run_sweep(**kwargs)
                self.assert_due(output, 0)
                self.assertEqual(len(calls), 1)

    def test_missing_update_still_checks_reopen_history(self):
        output, _ = self.run_sweep(updated_at=None, pages={1: (200, [
            {"event": "reopened", "created_at": stamp(NOW)},
        ])})
        self.assert_due(output, 0)

    def test_unreadable_or_malformed_history_does_not_select_for_closure(self):
        for response in ((403, None), (200, {}), (200, [None]),
                         (200, [{"event": "reopened", "created_at": "invalid"}])):
            with self.subTest(response=response):
                output, _ = self.run_sweep(pages={1: response})
                self.assert_due(output, 0)

    def test_full_page_requires_a_final_page_before_aging_the_pr(self):
        output, calls = self.run_sweep(pages={1: (200, [{"event": "labeled"}] * 100)})
        self.assert_due(output, 1)
        self.assertTrue(any(path.endswith("page=2") for path in calls))

    def test_expired_reopen_closes_with_configured_review_window_in_reply(self):
        for days in (3, 5):
            with self.subTest(window_days=days):
                output, _ = self.run_sweep(
                    created_at=stamp(NOW - timedelta(days=100)), dry=False,
                    extra_args=("--min-age", str(days)),
                    pages={1: (200, [{"event": "reopened",
                                     "created_at": stamp(NOW - timedelta(days=6))}])},
                )
                self.assert_due(output, 1)
                self.assertIn("DONE: closed=1 fails=0", output)
                self.assertEqual([item[1] for item in self.writes], ["PATCH", "POST"])
                comment = self.writes[1][2]["body"]
                self.assertIn(f"review window ({days} days)", comment)
                self.assertNotIn("100 days", comment)


if __name__ == "__main__":
    unittest.main()
