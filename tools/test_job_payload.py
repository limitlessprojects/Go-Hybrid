"""Offline tests of the source uploaded by kaggle_submit; no remote jobs."""
import ast
import contextlib
import importlib.util
import io
import json
import subprocess
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
    def test_submission_preserves_local_worker_files(self):
        # Run the old cleanup's real git checkout in an isolated repository:
        # mocking every shell call hides loss of uncommitted template changes.
        for outcome in (0, 1, 'timeout'):
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                worker = root / 'worker'
                worker.mkdir()
                script = worker / 'script.py'
                script.write_bytes((job_tool.INJECT_MARKER + '\n').encode())
                for argv in (['git', 'init', '-q'], ['git', 'add', 'worker/script.py'],
                             ['git', '-c', 'user.name=Fixture', '-c',
                              'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture']):
                    subprocess.run(argv, cwd=root, check=True, capture_output=True)
                local = (job_tool.INJECT_MARKER + '\r\n# local edit: caf\u00e9\r\n').encode('utf-8')
                script.write_bytes(local)
                sidecar = worker / 'job.json'
                sidecar.write_bytes(b'{"local":"keep these exact bytes"}\r\n')
                original_sidecar = sidecar.read_bytes()
                jobdir = root / 'input'
                jobdir.mkdir()
                (jobdir / 'job.json').write_text('{"job":"offline","commands":[]}', encoding='utf-8')

                def fake_sh(command, **kwargs):
                    if 'kernels push' in command:
                        self.assertIn(b'# local edit:', script.read_bytes())
                        self.assertEqual(json.loads(sidecar.read_bytes())['sha'], 'a' * 40)
                        if outcome == 'timeout':
                            raise subprocess.TimeoutExpired('offline push', 180)
                        return outcome, 'offline push fixture'
                    if command == 'git checkout -- worker/script.py':
                        subprocess.run(['git', 'checkout', '--', 'worker/script.py'],
                                       cwd=root, check=True, capture_output=True)
                        return 0, ''
                    self.fail('unexpected command: ' + command)

                with mock.patch.object(job_tool, 'WORKER_DIR', str(worker)), \
                     mock.patch.object(job_tool, 'REPO_DIR', str(root)), \
                     mock.patch.object(job_tool, 'resolve_head', return_value='a' * 40), \
                     mock.patch.object(job_tool, 'sh', side_effect=fake_sh), \
                     contextlib.redirect_stdout(io.StringIO()):
                    if outcome == 'timeout':
                        with self.assertRaises(subprocess.TimeoutExpired):
                            job_tool.kaggle_submit(str(jobdir))
                    elif outcome:
                        with self.assertRaises(SystemExit):
                            job_tool.kaggle_submit(str(jobdir))
                    else:
                        job_tool.kaggle_submit(str(jobdir))
                self.assertEqual(script.read_bytes(), local)
                self.assertEqual(sidecar.read_bytes(), original_sidecar)
                self.assertEqual((jobdir / 'submitted.json').exists(), outcome == 0)

    def test_sidecar_write_failure_restores_template(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            worker = root / 'worker'
            worker.mkdir()
            script = worker / 'script.py'
            pristine = (job_tool.INJECT_MARKER + '\n# local changes\n').encode()
            script.write_bytes(pristine)
            jobdir = root / 'input'
            jobdir.mkdir()
            (jobdir / 'job.json').write_text('{"job":"offline","commands":[]}', encoding='utf-8')
            real_open = open

            def fail_sidecar_write(path, mode='r', *args, **kwargs):
                if Path(path) == worker / 'job.json' and 'w' in mode:
                    raise OSError('simulated sidecar write failure')
                return real_open(path, mode, *args, **kwargs)

            with mock.patch.object(job_tool, 'WORKER_DIR', str(worker)), \
                 mock.patch.object(job_tool, 'resolve_head', return_value='a' * 40), \
                 mock.patch.object(job_tool, 'sh') as shell, \
                 mock.patch('builtins.open', side_effect=fail_sidecar_write):
                with self.assertRaisesRegex(OSError, 'sidecar write failure'):
                    job_tool.kaggle_submit(str(jobdir))
            shell.assert_not_called()
            self.assertEqual(script.read_bytes(), pristine)
            self.assertFalse((worker / 'job.json').exists())

    def submit(self, command, push_exit=0):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            worker = root / "worker"
            worker.mkdir()
            script = worker / "script.py"
            script.write_text(TEMPLATE, encoding="utf-8")
            jobdir = root / "input"
            jobdir.mkdir()
            original = {"job": "offline-payload", "sha": "HEAD", "commands": [
                {"name": "example", "run": command}]}
            (jobdir / "job.json").write_text(json.dumps(original))
            uploads = []

            def fake_sh(command, **kwargs):
                if "kernels push" in command:
                    uploads.append((script.read_text(encoding="utf-8"),
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

            self.assertEqual(script.read_text(encoding="utf-8"), TEMPLATE)
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
