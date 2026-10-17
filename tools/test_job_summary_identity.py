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
            nested = root / "result/raw/nested"
            nested.mkdir(parents=True)
            source = nested / "summary.json"
            source.write_text(payload, encoding="utf-8")
            dest = root / "result/summary.json"
            previous = b'{"previous": "reviewed result"}\n'
            dest.write_bytes(previous)
            if submitted is not MISSING:
                (root / "submitted.json").write_text(json.dumps(submitted), encoding="utf-8")
            with mock.patch.object(job, "sh", return_value=(0, "offline successful transfer")), \
                 contextlib.redirect_stdout(io.StringIO()):
                if valid:
                    job.kaggle_fetch(str(root), slug="fixture/kernel")
                else:
                    with self.assertRaises(SystemExit) as caught:
                        job.kaggle_fetch(str(root), slug="fixture/kernel")
                    self.assertNotEqual(caught.exception.code, 0)
            self.assertEqual(dest.read_bytes(), source.read_bytes() if valid else previous)
            self.assertEqual(source.read_text(encoding="utf-8"), payload)

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


if __name__ == "__main__":
    unittest.main()
