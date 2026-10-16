"""Regression tests for fail-closed CLI argument validation (no network)."""
import json
import shlex
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ENTRY = Path(__file__).with_name("job.py")


class ActionsDispatchArgumentTests(unittest.TestCase):
    def call_dispatch(self, *args):
        return subprocess.run(
            [sys.executable, str(ENTRY), "actions", "dispatch", *args],
            capture_output=True,
            text=True,
            timeout=5,
        )

    def test_missing_parallelism_reports_usage_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "3")
        self.assertEqual(result.returncode, 2)
        self.assertIn("<maxparallel>", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_invalid_parallelism_reports_error_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "3", "bad")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integers", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_nonpositive_values_are_rejected(self):
        for shards, parallel in [("0", "2"), ("3", "-1"), ("-2", "1")]:
            with self.subTest(shards=shards, parallel=parallel):
                result = self.call_dispatch("dummy.yml", "0123456", "demo", shards, parallel)
                self.assertEqual(result.returncode, 2)
                self.assertIn("positive integers", result.stderr)
                self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_invalid_shards_reports_error_not_traceback(self):
        result = self.call_dispatch("dummy.yml", "0123456", "demo", "bad", "2")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integers", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)


class ActionsWatchArgumentTests(unittest.TestCase):
    def call_watch(self, *args):
        return subprocess.run(
            [sys.executable, str(ENTRY), "actions", "watch", *args],
            capture_output=True,
            text=True,
            timeout=5,
        )

    def test_invalid_run_id_reports_error_not_traceback(self):
        result = self.call_watch("bad")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integer", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_nonpositive_run_id_reports_error_not_traceback(self):
        for run_id in ("0", "-5"):
            with self.subTest(run_id=run_id):
                result = self.call_watch(run_id)
                self.assertEqual(result.returncode, 2)
                self.assertIn("positive integer", result.stderr)
                self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_extra_arguments_report_usage(self):
        result = self.call_watch("123", "extra")
        self.assertEqual(result.returncode, 2)
        self.assertIn("[run_id]", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_watch_passes_run_id_argument(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.object(sys, "argv", ["job.py", "actions", "watch", "42"]), \
             mock.patch.object(job_mod, "actions_watch") as mock_watch:
            job_mod.main()
            mock_watch.assert_called_once_with(42)

    def test_watch_without_run_id_lists_runs(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.object(sys, "argv", ["job.py", "actions", "watch"]), \
             mock.patch.object(job_mod, "gh_api", return_value={"workflow_runs": []}) as api:
            job_mod.main()
            api.assert_called_once_with(
                "/repos/limitlessprojects/Go-Hybrid/actions/runs?per_page=5")

    def test_make_actions_watch_invokes_documented_cli(self):
        # Record the real recipe's arguments instead of calling GitHub.
        with tempfile.TemporaryDirectory() as tmp:
            recorder = Path(tmp) / "record arguments.py"
            recorder.write_text("import json, sys; print(json.dumps(sys.argv[1:]))\n")
            python = "{} {}".format(shlex.quote(sys.executable), shlex.quote(str(recorder)))
            result = subprocess.run(
                ["make", "-s", "--no-print-directory", "actions-watch", "PY=" + python],
                cwd=ENTRY.resolve().parents[1],
                capture_output=True,
                text=True,
                timeout=5,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(json.loads(result.stdout), ["tools/job.py", "actions", "watch"])


class ActionsFetchArgumentTests(unittest.TestCase):
    def call_fetch(self, *args):
        return subprocess.run(
            [sys.executable, str(ENTRY), "actions", "fetch", *args],
            capture_output=True,
            text=True,
            timeout=5,
        )

    def test_invalid_run_id_reports_error_not_traceback(self):
        result = self.call_fetch("bad", "/tmp/results")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive integer", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_nonpositive_run_id_reports_error_not_traceback(self):
        for run_id in ("0", "-1"):
            with self.subTest(run_id=run_id):
                result = self.call_fetch(run_id, "/tmp/results")
                self.assertEqual(result.returncode, 2)
                self.assertIn("positive integer", result.stderr)
                self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_missing_token_fails_closed(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.object(job_mod, "gh_token", return_value=None), \
             mock.patch.object(job_mod, "gh_api", return_value={"artifacts": [{"archive_download_url": "http://example", "name": "a", "size_in_bytes": 1}]}), \
             self.assertRaises(SystemExit) as ctx:
            job_mod.actions_fetch(12345, "/tmp/results")
        self.assertTrue(bool(ctx.exception.code))


class KaggleArgumentTests(unittest.TestCase):
    def call_kaggle(self, action, *args):
        return subprocess.run(
            [sys.executable, str(ENTRY), "kaggle", action, *args],
            capture_output=True,
            text=True,
            timeout=5,
        )

    def test_submit_missing_jobdir_fails_closed(self):
        result = self.call_kaggle("submit", "/nonexistent/jobdir")
        self.assertEqual(result.returncode, 2)
        self.assertIn("job.json", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_wait_invalid_minutes_fails_closed(self):
        result = self.call_kaggle("wait", "bad")
        self.assertEqual(result.returncode, 2)
        self.assertIn("positive number of minutes", result.stderr)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_wait_nonpositive_minutes_fails_closed(self):
        for minutes in ("0", "-5", "nan", "inf", "-inf"):
            with self.subTest(minutes=minutes):
                result = self.call_kaggle("wait", minutes)
                self.assertEqual(result.returncode, 2)
                self.assertIn("positive number of minutes", result.stderr)
                self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_wait_valid_minutes_passes(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.object(sys, "argv", ["job.py", "kaggle", "wait", "10.5", "test-slug"]), \
             mock.patch.object(job_mod, "kaggle_wait", return_value=0) as mock_wait:
            with self.assertRaises(SystemExit) as ctx:
                job_mod.main()
            self.assertEqual(ctx.exception.code, 0)
            mock_wait.assert_called_once_with(10.5, "test-slug")

    def test_status_extra_arguments_reports_usage(self):
        result = self.call_kaggle("status", "slug", "extra")
        self.assertEqual(result.returncode, 2)
        self.assertIn("[slug]", result.stdout)
        self.assertNotIn("Traceback", result.stdout + result.stderr)

    def test_status_passes_slug_argument(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.object(sys, "argv", ["job.py", "kaggle", "status", "custom-slug"]), \
             mock.patch.object(job_mod, "kaggle_status") as mock_status:
            job_mod.main()
            mock_status.assert_called_once_with("custom-slug")


class GhTokenTests(unittest.TestCase):
    def test_reads_token_from_environment(self):
        import importlib.util
        from unittest import mock
        spec = importlib.util.spec_from_file_location("job_module", ENTRY)
        job_mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job_mod)
        with mock.patch.dict("os.environ", {"GITHUB_TOKEN": "token_from_env"}):
            self.assertEqual(job_mod.gh_token(), "token_from_env")
        with mock.patch.dict("os.environ", {"GITHUB_TOKEN": "", "GH_TOKEN": "token_from_gh"}):
            self.assertEqual(job_mod.gh_token(), "token_from_gh")


if __name__ == "__main__":
    unittest.main()
