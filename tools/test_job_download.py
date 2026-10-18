"""Offline archive download integrity tests; no GitHub requests or real tokens."""
import contextlib
import http.client
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("download_job", Path(__file__).with_name("job.py"))
job = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job)


class DownloadIntegrityTests(unittest.TestCase):
    def fetch(self, directory, response):
        artifact = {"name": "results", "archive_download_url": "https://example.invalid/archive",
                    "size_in_bytes": 9}
        with mock.patch.object(job, "gh_api", return_value={"artifacts": [artifact]}), \
             mock.patch.object(job, "gh_token", return_value="offline-fixture"), \
             mock.patch.object(job.urllib.request, "urlopen", return_value=response), \
             contextlib.redirect_stdout(io.StringIO()):
            job.actions_fetch(123, str(directory))

    def test_interrupted_read_preserves_existing_archive(self):
        for existing in (None, b"previous validated archive"):
            with self.subTest(existing=existing), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                result = root / "result"
                result.mkdir()
                dest = result / "results.zip"
                if existing is not None:
                    dest.write_bytes(existing)
                response = mock.MagicMock()
                response.__enter__.return_value = response
                response.read.side_effect = http.client.IncompleteRead(b"partial", 100)
                with self.assertRaises(http.client.IncompleteRead):
                    self.fetch(root, response)
                self.assertEqual(dest.read_bytes() if dest.exists() else None, existing)
                self.assertEqual(sorted(p.name for p in result.iterdir()),
                                 [] if existing is None else ["results.zip"])

    def test_success_replaces_archive_only_after_response_read(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = root / "result"
            result.mkdir()
            dest = result / "results.zip"
            dest.write_bytes(b"old")
            observed = []

            class Response(io.BytesIO):
                def read(self, *args):
                    observed.append(dest.read_bytes())
                    return super().read(*args)

            self.fetch(root, Response(b"new complete archive"))
            self.assertTrue(observed)
            self.assertTrue(all(data == b"old" for data in observed))
            self.assertEqual(dest.read_bytes(), b"new complete archive")
            self.assertEqual([p.name for p in result.iterdir()], ["results.zip"])

    def test_publish_failure_keeps_previous_archive_and_cleans_temporary_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            result = root / "result"
            result.mkdir()
            dest = result / "results.zip"
            dest.write_bytes(b"old")
            with mock.patch.object(job.os, "replace", side_effect=OSError("fixture publish failure")):
                with self.assertRaisesRegex(OSError, "fixture publish failure"):
                    self.fetch(root, io.BytesIO(b"new"))
            self.assertEqual(dest.read_bytes(), b"old")
            self.assertEqual([p.name for p in result.iterdir()], ["results.zip"])

    def test_success_creates_new_archive(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fetch(root, io.BytesIO(b"complete archive"))
            self.assertEqual((root / "result/results.zip").read_bytes(), b"complete archive")
            self.assertEqual([p.name for p in (root / "result").iterdir()], ["results.zip"])


if __name__ == "__main__":
    unittest.main()
