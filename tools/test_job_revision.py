"""Submit-time revision checks, using only local repositories and fake uploads."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock


spec = importlib.util.spec_from_file_location("revision_job", Path(__file__).with_name("job.py"))
job = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job)


class SubmitRevisionTests(unittest.TestCase):
    def test_failed_git_lookup_aborts_before_payload_or_upload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            worker = root / "worker"
            worker.mkdir()
            template = (job.INJECT_MARKER + "\n").encode()
            (worker / "script.py").write_bytes(template)
            (root / "job.json").write_text(json.dumps({"job": "offline", "commands": []}))
            uploads = []

            def fake_sh(command, **kwargs):
                if command.startswith("git rev-parse"):
                    return 128, "fatal: HEAD is not a valid revision"
                if "kernels push" in command:
                    uploads.append(command)
                return 0, "fixture"

            with mock.patch.object(job, "WORKER_DIR", str(worker)), \
                 mock.patch.object(job, "sh", side_effect=fake_sh), \
                 contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as raised:
                    job.kaggle_submit(str(root))
            self.assertNotEqual(raised.exception.code, 0)
            self.assertEqual(uploads, [])
            self.assertEqual((worker / "script.py").read_bytes(), template)
            self.assertFalse((worker / "job.json").exists())
            self.assertFalse((root / "submitted.json").exists())

    def test_unborn_local_repository_cannot_supply_a_pin(self):
        with tempfile.TemporaryDirectory() as tmp:
            subprocess.run(["git", "init", "-q", tmp], check=True, capture_output=True)

            def local_git(command, **kwargs):
                # Run only the fixed revision lookup against a disposable repo.
                result = subprocess.run(command.split(), cwd=tmp, capture_output=True, text=True)
                return result.returncode, result.stdout + result.stderr

            with mock.patch.object(job, "sh", side_effect=local_git):
                with self.assertRaises(SystemExit):
                    job.resolve_head()

    def test_committed_local_repository_resolves_exact_head(self):
        with tempfile.TemporaryDirectory() as tmp:
            subprocess.run(["git", "init", "-q", tmp], check=True, capture_output=True)
            subprocess.run(["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                            "commit", "--allow-empty", "-qm", "fixture"], cwd=tmp,
                           check=True, capture_output=True)
            expected = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=tmp, text=True).strip()

            def local_git(command, **kwargs):
                result = subprocess.run(command.split(), cwd=tmp, capture_output=True, text=True)
                return result.returncode, result.stdout + result.stderr

            with mock.patch.object(job, "sh", side_effect=local_git):
                self.assertEqual(job.resolve_head(), expected)


if __name__ == "__main__":
    unittest.main()
