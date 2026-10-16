"""Offline tests of the source uploaded by kaggle_submit; no remote jobs."""
import ast
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("job_tool", ROOT / "tools/job.py")
job_tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(job_tool)
TEMPLATE = (ROOT / "worker/script.py").read_text(encoding="utf-8")


class KagglePayloadTests(unittest.TestCase):
    def submit(self, command, push_exit=0):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            worker = root / "worker"
            worker.mkdir()
            script = worker / "script.py"
            script.write_text(TEMPLATE)
            jobdir = root / "input"
            jobdir.mkdir()
            original = {"job": "offline-payload", "sha": "HEAD", "commands": [
                {"name": "example", "run": command}]}
            (jobdir / "job.json").write_text(json.dumps(original))
            uploads = []

            def fake_sh(command, **kwargs):
                if "kernels push" in command:
                    uploads.append((script.read_text(),
                                    (worker / "job.json").read_text()))
                    return push_exit, "offline push fixture"
                # Do not run git checkout, a CLI, or any network operation.
                return 0, ""

            with mock.patch.object(job_tool, "WORKER_DIR", str(worker)), \
                 mock.patch.object(job_tool, "resolve_head", return_value="a" * 40), \
                 mock.patch.object(job_tool, "sh", side_effect=fake_sh), \
                 contextlib.redirect_stdout(io.StringIO()):
                if push_exit:
                    with self.assertRaises(SystemExit):
                        job_tool.kaggle_submit(str(jobdir))
                else:
                    job_tool.kaggle_submit(str(jobdir))

            self.assertEqual(script.read_text(), TEMPLATE)
            self.assertFalse((worker / "job.json").exists())
            self.assertEqual((jobdir / "submitted.json").exists(), not push_exit)
            self.assertEqual(len(uploads), 1)
            source, sidecar = uploads[0]
            tree = ast.parse(source)
            assignment = next(node for node in tree.body
                              if isinstance(node, ast.Assign)
                              and any(isinstance(target, ast.Name)
                                      and target.id == "JOB_JSON"
                                      for target in node.targets))
            embedded = json.loads(ast.literal_eval(assignment.value))
            expected = dict(original, sha="a" * 40)
            self.assertEqual(embedded, expected)
            self.assertEqual(json.loads(sidecar), expected)

    def test_plain_command_round_trips(self):
        self.submit("printf 'hello\\n'")

    def test_triple_quoted_python_command_round_trips(self):
        self.submit("python3 -c \"print('''hello''')\"")

    def test_quotes_unicode_backslashes_and_newlines_round_trip(self):
        self.submit('python3 -c \'print("""caf\u00e9\\nC:\\\\temp""")\'\n# second line')

    def test_failed_push_restores_template_and_does_not_record_submission(self):
        self.submit("python3 -c \"print('''hello''')\"", push_exit=1)


if __name__ == "__main__":
    unittest.main()
