"""Regression tests for fail-closed CLI argument validation (no network)."""
import subprocess
import sys
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



class ActionsArtifactPaginationTests(unittest.TestCase):
    """Exercise complete ZIP retrieval against the documented page defaults."""
    def exercise_fetch(self, count, failing_page=None, failing_download=False):
        import contextlib
        import importlib.util
        import io
        import tempfile
        import zipfile
        from urllib.parse import parse_qs, urlparse
        from unittest.mock import patch

        spec = importlib.util.spec_from_file_location("job_pagination_fixture", ENTRY)
        job = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(job)
        api_pages, downloaded = [], []
        artifacts, bodies = [], {}
        for index in range(count):
            body = io.BytesIO()
            with zipfile.ZipFile(body, "w") as archive:
                archive.writestr("evidence.txt", "artifact-%d" % index)
            bodies[index] = body.getvalue()
            artifacts.append({"id": index, "name": "shard-%03d" % index,
                              "size_in_bytes": len(bodies[index]),
                              "archive_download_url": "https://artifacts.invalid/%d" % index})

        def api(path):
            query = parse_qs(urlparse(path).query)
            size = int(query.get("per_page", ["30"])[0])
            page = int(query.get("page", ["1"])[0])
            api_pages.append(page)
            if page == failing_page:
                raise RuntimeError("synthetic listing failure")
            start = (page - 1) * size
            return {"total_count": count, "artifacts": artifacts[start:start + size]}

        def download(req, timeout):
            self.assertEqual(timeout, 120)
            if failing_download:
                raise OSError("synthetic download failure")
            index = int(urlparse(req.full_url).path.rsplit("/", 1)[1])
            downloaded.append(index)
            return io.BytesIO(bodies[index])

        with tempfile.TemporaryDirectory() as directory:
            with patch.object(job, "gh_api", side_effect=api), \
                    patch.object(job, "gh_token", return_value="synthetic-fixture"), \
                    patch.object(job.urllib.request, "urlopen", side_effect=download), \
                    contextlib.redirect_stdout(io.StringIO()):
                if failing_page is not None:
                    with self.assertRaisesRegex(RuntimeError, "listing failure"):
                        job.actions_fetch(123, directory)
                    self.assertEqual(downloaded, [])
                    self.assertFalse((Path(directory) / "result").exists())
                    return
                if failing_download:
                    with self.assertRaisesRegex(OSError, "download failure"):
                        job.actions_fetch(123, directory)
                    return
                job.actions_fetch(123, directory)
            files = sorted((Path(directory) / "result").glob("*.zip"))
            self.assertEqual(len(files), count)
            self.assertEqual(downloaded, list(range(count)))
            for index, file in enumerate(files):
                self.assertEqual(file.read_bytes(), bodies[index])
                with zipfile.ZipFile(file) as archive:
                    self.assertEqual(archive.read("evidence.txt").decode(), "artifact-%d" % index)
            self.assertEqual(api_pages, list(range(1, count // 100 + 2)))

    def test_retrieves_more_than_default_thirty_artifacts(self):
        self.exercise_fetch(31)

    def test_retrieves_later_pages(self):
        self.exercise_fetch(101)

    def test_empty_small_and_exact_page_boundaries(self):
        for count in (0, 1, 30, 100, 200):
            with self.subTest(count=count):
                self.exercise_fetch(count)

    def test_later_page_error_does_not_start_partial_download(self):
        self.exercise_fetch(101, failing_page=2)

    def test_download_errors_still_propagate(self):
        self.exercise_fetch(1, failing_download=True)


if __name__ == "__main__":
    unittest.main()
