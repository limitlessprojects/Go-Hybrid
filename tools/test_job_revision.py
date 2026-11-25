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


tb_spec = importlib.util.spec_from_file_location("triage_bot", Path(__file__).with_name("triage_bot.py"))
triage_bot = importlib.util.module_from_spec(tb_spec)
tb_spec.loader.exec_module(triage_bot)


class TriageBotCompareCacheTests(unittest.TestCase):
    def test_collect_caches_shared_merge_base_and_skips_head_at_main(self):
        calls = []

        class FakeGh:
            repo = "o/r"

            def paginate(self, path, **kw):
                if path.startswith("/repos/o/r/pulls?"):
                    return [
                        {"number": i, "title": f"PR {i}", "user": {"login": "u"},
                         "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-01T00:00:00Z",
                         "head": {"sha": "mainsha00000" if i == 1 else f"headsha{i:05d}"}}
                        for i in range(1, 5)
                    ]
                return [{"filename": "compiler/crates/hy-check/src/buildable.rs", "additions": 10, "deletions": 1}]

            def get(self, path):
                calls.append(path)
                if path.endswith("/commits/main"):
                    return {"sha": "mainsha00000"}
                if "/compare/mainsha00000..." in path:
                    return {"behind_by": 4, "merge_base_commit": {"sha": "sharedmb0000"}}
                if "/compare/sharedmb0000...mainsha00000" in path:
                    return {"files": [{"filename": "compiler/crates/hy-check/src/buildable.rs"}]}
                return {"mergeable": True, "mergeable_state": "clean"}

        with contextlib.redirect_stdout(io.StringIO()):
            items, main_sha = triage_bot.collect({}, FakeGh(), type("A", (), {"jobs": 4})())
        self.assertEqual(main_sha, "mainsha00000")
        by_num = {it["number"]: it for it in items}
        self.assertEqual(by_num[1]["behind_by"], 0)
        self.assertEqual(by_num[2]["behind_by"], 4)
        self.assertEqual(by_num[2]["overlap_risk"], ["compiler/crates/hy-check/src/buildable.rs"])
        self.assertFalse(any("compare/mainsha00000...mainsha00000" in c for c in calls))
        self.assertEqual(sum(1 for c in calls if "compare/sharedmb0000...mainsha00000" in c), 1)


if __name__ == "__main__":
    unittest.main()
