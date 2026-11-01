"""Offline fetch tests: a successful transfer is not proof of result identity."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("summary_job", Path(__file__).with_name("job.py"))
job = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job)
PIN = {"job": "wanted-job", "sha": "a" * 40}
MISSING = object()


class SummaryIdentityTests(unittest.TestCase):
    def fetch(self, payload, submitted=PIN, valid=True):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dest = root / "result/summary.json"
            dest.parent.mkdir(parents=True, exist_ok=True)
            previous = b'{"previous": "reviewed result"}\n'
            dest.write_bytes(previous)
            if submitted is not MISSING:
                (root / "submitted.json").write_text(json.dumps(submitted), encoding="utf-8")

            # The staged download delivers the summary; only this invocation's
            # files are promotion candidates (#478), and the cache copy keeps
            # the fetched bytes for diagnosis.
            def offline_download(command, **kwargs):
                staging = next((root / "result/raw").glob(".staging-*"))
                (staging / "summary.json").write_bytes(payload.encode("utf-8"))
                return 0, "offline successful transfer"

            with mock.patch.object(job, "sh", side_effect=offline_download), \
                 contextlib.redirect_stdout(io.StringIO()):
                if valid:
                    job.kaggle_fetch(str(root), slug="fixture/kernel")
                else:
                    with self.assertRaises(SystemExit) as caught:
                        job.kaggle_fetch(str(root), slug="fixture/kernel")
                    self.assertNotEqual(caught.exception.code, 0)
            self.assertEqual(
                dest.read_bytes(),
                payload.encode("utf-8") if valid else previous)
            self.assertEqual(
                (root / "result/raw/summary.json").read_text(encoding="utf-8"),
                payload)

    def test_wrong_job_or_revision_does_not_replace_reviewed_summary(self):
        for field, wrong in (("job", "different-job"), ("sha", "b" * 40)):
            with self.subTest(field=field):
                self.fetch(json.dumps(dict(PIN, **{field: wrong}, ok=True)), valid=False)

    def test_missing_identity_does_not_replace_reviewed_summary(self):
        for field in PIN:
            with self.subTest(field=field):
                summary = dict(PIN, ok=True)
                del summary[field]
                self.fetch(json.dumps(summary), valid=False)

    def test_malformed_report_does_not_replace_reviewed_summary(self):
        for payload in ("{broken json", "null", "[]"):
            with self.subTest(payload=payload):
                self.fetch(payload, valid=False)

    def test_invalid_submission_record_does_not_disable_identity_check(self):
        for submitted in ({}, [], {"job": "wanted-job"}, dict(PIN, sha=None)):
            with self.subTest(submitted=submitted):
                self.fetch(json.dumps(dict(PIN, ok=True)), submitted=submitted, valid=False)

    def test_matching_successful_result_is_copied(self):
        self.fetch(json.dumps(dict(PIN, ok=True), indent=2) + "\n")

    def test_matching_failed_result_is_still_available_for_diagnosis(self):
        self.fetch(json.dumps(dict(PIN, ok=False, steps=[{"exit": 1}])))

    def test_fetch_without_local_submission_record_remains_supported(self):
        self.fetch(json.dumps({"job": "external-job", "sha": "c" * 40, "ok": True}),
                   submitted=MISSING)


class SummaryPublicationTests(unittest.TestCase):
    def publish(self, failure=None, existing=True, ok=True):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            dest = root / "result/summary.json"
            dest.parent.mkdir()
            previous = b'{"previous": "reviewed evidence"}\n'
            if existing:
                dest.write_bytes(previous)
            payload = json.dumps(dict(PIN, ok=ok, note="survey: 東京"),
                                 ensure_ascii=False).encode("utf-8")
            (root / "submitted.json").write_text(json.dumps(PIN), encoding="utf-8")

            def download(command, **kwargs):
                staging = next((root / "result/raw").glob(".staging-*"))
                (staging / "summary.json").write_bytes(payload)
                return 0, "offline download"

            original_copyfile = job.shutil.copyfile
            original_copyfileobj = job.shutil.copyfileobj

            def broken_copy(source, destination, *args, **kwargs):
                # Exercise a real partial write, including the old copyfile
                # boundary, rather than only throwing before touching output.
                if hasattr(destination, "write"):
                    if not Path(destination.name).name.startswith(".summary-"):
                        return original_copyfileobj(source, destination, *args, **kwargs)
                    destination.write(b'{"partial":')
                else:
                    if Path(destination) != dest:
                        return original_copyfile(source, destination, *args, **kwargs)
                    Path(destination).write_bytes(b'{"partial":')
                raise OSError("fixture disk full")

            with mock.patch.object(job, "sh", side_effect=download), \
                 contextlib.redirect_stdout(io.StringIO()), \
                 contextlib.redirect_stderr(io.StringIO()), \
                 contextlib.ExitStack() as stack:
                if failure == "copy":
                    stack.enter_context(mock.patch.object(job.shutil, "copyfile", side_effect=broken_copy))
                    stack.enter_context(mock.patch.object(job.shutil, "copyfileobj", side_effect=broken_copy))
                elif failure == "replace":
                    stack.enter_context(mock.patch.object(job.os, "replace", side_effect=OSError("fixture publish failure")))
                try:
                    result = job.kaggle_fetch(str(root), slug="fixture/kernel")
                except OSError:
                    # Older code propagates a write error after truncating the
                    # destination. Still inspect the bytes to prove that loss.
                    result = 1

            self.assertEqual(result, 1 if failure else 0)
            self.assertEqual(dest.read_bytes() if dest.exists() else None,
                             (previous if existing else None) if failure else payload)
            self.assertEqual((root / "result/raw/summary.json").read_bytes(), payload)
            self.assertEqual(sorted(p.name for p in dest.parent.iterdir()),
                             ["raw", "summary.json"] if dest.exists() else ["raw"])

    def test_partial_copy_keeps_reviewed_summary(self):
        self.publish("copy")

    def test_partial_copy_does_not_create_a_corrupt_summary(self):
        self.publish("copy", existing=False)

    def test_failed_replace_keeps_reviewed_summary(self):
        self.publish("replace")

    def test_failed_replace_does_not_create_summary(self):
        self.publish("replace", existing=False)

    def test_success_publishes_complete_utf8_summary(self):
        self.publish()

    def test_success_creates_missing_summary(self):
        self.publish(existing=False)

    def test_failed_job_report_can_still_be_published(self):
        self.publish(ok=False)


if __name__ == "__main__":
    unittest.main()
