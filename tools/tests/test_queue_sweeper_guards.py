#!/usr/bin/env python3
"""Offline unit tests for tools/queue_sweeper.py: script-relative triage_config.json
resolution, --min-age/--max argument validation, and strict close/comment HTTP
status checking."""

import contextlib
from datetime import datetime, timedelta, timezone
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import queue_sweeper as sweeper  # noqa: E402


NOW = datetime(2026, 10, 24, 15, tzinfo=timezone.utc)


class FixedDateTime(datetime):
    @classmethod
    def now(cls, tz=None):
        return NOW if tz is not None else NOW.replace(tzinfo=None)


class QueueSweeperGuardsTests(unittest.TestCase):
    def test_config_protected_numbers_resolves_outside_repo_root_and_validates_schema(self):
        with tempfile.TemporaryDirectory() as td:
            old_cwd = os.getcwd()
            try:
                os.chdir(td)
                nums = sweeper.config_protected_numbers()
                self.assertIn(336, nums)
                self.assertIn(465, nums)
            finally:
                os.chdir(old_cwd)

            bad_cfg = Path(td) / "bad_triage_config.json"
            bad_cfg.write_text(json.dumps({"wildcard_holds": ["not-an-int"]}), encoding="utf-8")
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                self.assertEqual(sweeper.config_protected_numbers(bad_cfg), set())
            self.assertIn("must be a list of integers", out.getvalue())

    def test_nonpositive_min_age_and_negative_max_fail_closed(self):
        for bad_args in (["queue_sweeper", "--min-age", "0"],
                         ["queue_sweeper", "--min-age", "-3"],
                         ["queue_sweeper", "--max", "-1"]):
            with self.subTest(argv=bad_args), \
                 patch.object(sys, "argv", bad_args), \
                 contextlib.redirect_stderr(io.StringIO()), \
                 self.assertRaises(SystemExit) as cm:
                sweeper.main()
            self.assertEqual(cm.exception.code, 2)

    def test_close_422_and_comment_403_count_as_failures_not_closes(self):
        old_stamp = (NOW - timedelta(days=20)).isoformat().replace("+00:00", "Z")
        pr = {
            "number": 99,
            "created_at": old_stamp,
            "updated_at": old_stamp,
            "labels": [],
            "title": "Stale PR",
            "user": {"login": "alice"},
        }
        writes = []

        def api_patch_422(path, method="GET", body=None):
            if method != "GET":
                writes.append((path, method))
                return 422, None
            if path.startswith("/repos/example/project/pulls?"):
                return 200, [pr]
            return 200, {"open_issues_count": 1}

        out = io.StringIO()
        with patch.object(sweeper, "TOK", "tok"), \
             patch.object(sweeper, "REPO", "example/project"), \
             patch.object(sweeper, "datetime", FixedDateTime), \
             patch.object(sweeper, "api", side_effect=api_patch_422), \
             patch.object(sweeper.time, "sleep"), \
             patch.object(sweeper, "config_protected_numbers", return_value=set()), \
             patch.object(sys, "argv", ["queue_sweeper"]), \
             contextlib.redirect_stdout(out):
            sweeper.main()
        self.assertIn("DONE: closed=0 fails=1", out.getvalue())
        self.assertEqual(writes, [("/repos/example/project/pulls/99", "PATCH")])

        writes.clear()

        def api_comment_403(path, method="GET", body=None):
            if method == "PATCH":
                writes.append((path, method))
                return 200, {"state": "closed"}
            if method == "POST":
                writes.append((path, method))
                return 403, None
            if path.startswith("/repos/example/project/pulls?"):
                return 200, [pr]
            return 200, {"open_issues_count": 0}

        out2 = io.StringIO()
        with patch.object(sweeper, "TOK", "tok"), \
             patch.object(sweeper, "REPO", "example/project"), \
             patch.object(sweeper, "datetime", FixedDateTime), \
             patch.object(sweeper, "api", side_effect=api_comment_403), \
             patch.object(sweeper.time, "sleep"), \
             patch.object(sweeper, "config_protected_numbers", return_value=set()), \
             patch.object(sys, "argv", ["queue_sweeper"]), \
             contextlib.redirect_stdout(out2):
            sweeper.main()
        self.assertIn("DONE: closed=0 fails=1", out2.getvalue())


if __name__ == "__main__":
    unittest.main()
