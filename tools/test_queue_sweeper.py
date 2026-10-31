"""Unit tests for tools/queue_sweeper.py (offline, stdlib only).

Verifies accurate accounting of PATCH closure status and comment delivery
responses (fixes #2107).
"""

import contextlib
import io
import unittest
from unittest.mock import patch

import queue_sweeper as qs


class QueueSweeperStatusTests(unittest.TestCase):
    def _run_sweeper(self, patch_status, post_status, get_pr_state="open"):
        calls = []

        def mock_api(path, method="GET", body=None):
            calls.append({"path": path, "method": method, "body": body})
            if "/pulls?" in path:
                return 200, [{"number": 42, "created_at": "2020-01-01T00:00:00Z", "labels": []}]
            if method == "PATCH":
                return patch_status, None
            if method == "POST":
                return post_status, None
            if "/pulls/42" in path and method == "GET":
                return 200, {"number": 42, "state": get_pr_state}
            return 200, {"open_issues_count": 1}

        out = io.StringIO()
        with patch.object(qs, "TOK", "test-token"), \
             patch.object(qs, "api", side_effect=mock_api), \
             patch.object(qs, "config_protected_numbers", return_value=set()), \
             patch.object(qs.time, "sleep"), \
             patch("sys.argv", ["queue_sweeper.py"]), \
             contextlib.redirect_stdout(out):
            qs.main()

        return out.getvalue(), calls

    def test_patch_200_and_post_201_reports_full_success(self):
        output, calls = self._run_sweeper(200, 201)
        self.assertIn("closed=1 fails=0 comment_fails=0", output)
        methods = [c["method"] for c in calls]
        self.assertEqual(methods.count("PATCH"), 1)
        self.assertEqual(methods.count("POST"), 1)

    def test_patch_422_unconfirmed_reports_failure(self):
        """Unconfirmed PATCH 422 does not close the PR; report as fail without commenting."""
        output, calls = self._run_sweeper(422, 201, get_pr_state="open")
        self.assertIn("closed=0 fails=1", output)
        methods = [c["method"] for c in calls]
        self.assertEqual(methods.count("PATCH"), 1)
        self.assertEqual(methods.count("POST"), 0)

    def test_patch_422_confirmed_closed_reports_success(self):
        """If PATCH 422 occurred because the PR is already closed, verify state and deliver comment."""
        output, calls = self._run_sweeper(422, 201, get_pr_state="closed")
        self.assertIn("closed=1 fails=0 comment_fails=0", output)
        methods = [c["method"] for c in calls]
        self.assertEqual(methods.count("PATCH"), 1)
        self.assertEqual(methods.count("POST"), 1)

    def test_patch_200_and_post_422_decouples_comment_failure(self):
        """Successful closure with rejected comment tracks comment_fails separately."""
        output, _ = self._run_sweeper(200, 422)
        self.assertIn("closed=1 fails=0 comment_fails=1", output)

    def test_patch_500_reports_closure_failure(self):
        output, calls = self._run_sweeper(500, 201)
        self.assertIn("closed=0 fails=1", output)
        methods = [c["method"] for c in calls]
        self.assertEqual(methods.count("PATCH"), 1)
        self.assertEqual(methods.count("POST"), 0)

    def test_patch_200_and_post_403_tolerates_comment_restriction(self):
        """403 on comment (e.g. interaction restriction) does not flag comment failure."""
        output, _ = self._run_sweeper(200, 403)
        self.assertIn("closed=1 fails=0 comment_fails=0", output)


if __name__ == "__main__":
    unittest.main()
