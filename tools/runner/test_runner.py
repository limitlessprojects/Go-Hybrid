"""Verify hard/soft job failures with real, harmless subprocess commands."""
import contextlib
import importlib.util
import io
import json
import os
import pathlib
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    'runner', pathlib.Path(__file__).with_name('runner.py'))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


def step(name, code, hard=False, timeout=10):
    return {'name': name, 'run': [sys.executable, '-c', code],
            'hard': hard, 'timeout': timeout}


class RunnerFailureTests(unittest.TestCase):
    def run_job(self, setup, commands):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            job_path = root / 'job.json'
            results = root / 'results'
            job_path.write_text(json.dumps({'job': 'test', 'setup': setup,
                                          'commands': commands}), encoding='utf-8')
            argv = ['runner.py', str(job_path), '--results-dir', str(results),
                    '--repo-dir', str(root / 'no-checkout')]
            # Keep tests isolated: no hardware probes, tokens or persistent env edits.
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            return stopped.exception.code, summary

    def test_hard_setup_failure_skips_other_setup_and_commands(self):
        code, summary = self.run_job(
            [step('required', 'raise SystemExit(7)', True), step('later-setup', 'print(1)')],
            [step('must-not-run', 'print("SIDE EFFECT")')])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [('required', 7)])
        self.assertIsNotNone(summary['finished'])

    def test_soft_setup_failure_continues_and_stays_failed(self):
        code, summary = self.run_job(
            [step('optional', 'raise SystemExit(7)'), step('later-setup', 'print(1)')],
            [step('command', 'print(2)')])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual([s['name'] for s in summary['steps']], ['optional', 'later-setup', 'command'])

    def test_successful_hard_setup_allows_commands(self):
        code, summary = self.run_job([step('required', 'print(1)', True)], [step('command', 'print(2)')])
        self.assertEqual(code, 0)
        self.assertTrue(summary['ok'])
        self.assertEqual([s['name'] for s in summary['steps']], ['required', 'command'])

    def test_hard_command_failure_stops_later_commands(self):
        code, summary = self.run_job([], [step('first', 'raise SystemExit(9)', True), step('later', 'print(1)')])
        self.assertEqual(code, 1)
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [('first', 9)])

    def test_soft_command_failure_continues(self):
        code, summary = self.run_job([], [step('first', 'raise SystemExit(9)'), step('later', 'print(1)')])
        self.assertEqual(code, 1)
        self.assertEqual([s['name'] for s in summary['steps']], ['first', 'later'])

    def test_hard_setup_timeout_stops_commands_and_records_timeout(self):
        code, summary = self.run_job([step('required', 'import time; time.sleep(60)', True, 0.1)],
                                     [step('must-not-run', 'print(1)')])
        self.assertEqual(code, 1)
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [('required', 124)])

    def test_prior_success_is_preserved_after_hard_setup_failure(self):
        code, summary = self.run_job([step('prior', 'print(1)'), step('required', 'raise SystemExit(7)', True)],
                                     [step('must-not-run', 'print(2)')])
        self.assertEqual(code, 1)
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [('prior', 0), ('required', 7)])


class CommandContractTests(unittest.TestCase):
    def test_success_merges_output_and_preserves_status(self):
        code, duration, output = runner.run_cmd(
            [sys.executable, '-c',
             'import sys; print("out", flush=True); sys.stderr.write("err\\n"); '
             'raise SystemExit(7)'], 5)
        self.assertEqual(code, 7)
        self.assertEqual(output.splitlines(), ['out', 'err'])
        self.assertGreaterEqual(duration, 0)

    def test_cwd_environment_and_output_tail_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            env = dict(os.environ, RUNNER_TEST_VALUE='probe')
            code, _, output = runner.run_cmd(
                [sys.executable, '-c',
                 'import os; print(os.getcwd()); print(os.environ["RUNNER_TEST_VALUE"])'],
                5, cwd=directory, env=env)
            self.assertEqual(code, 0)
            self.assertEqual(pathlib.Path(output.splitlines()[0]).resolve(),
                             pathlib.Path(directory).resolve())
            self.assertEqual(output.splitlines()[1], 'probe')
        code, _, output = runner.run_cmd(
            [sys.executable, '-c', 'print("x" * 5000, end="")'], 5)
        self.assertEqual(code, 0)
        self.assertEqual(output, 'x' * 4000)

    def test_missing_executable_stays_runner_error(self):
        with tempfile.TemporaryDirectory() as directory:
            code, _, output = runner.run_cmd([str(pathlib.Path(directory) / 'missing')], 5)
        self.assertEqual(code, 125)
        self.assertIn('runner error', output)

    def test_invalid_utf8_output_is_replaced(self):
        code, _, output = runner.run_cmd(
            [sys.executable, '-c', 'import sys; sys.stdout.buffer.write(b"\\xff")'], 5)
        self.assertEqual(code, 0)
        self.assertEqual(output, '\ufffd')


@unittest.skipUnless(os.name == 'posix', 'process-group cleanup is POSIX-only')
class PosixCommandTimeoutTests(unittest.TestCase):
    def assert_descendant_stops(self, wait_for_child):
        # The child waits for a marker sent only AFTER run_cmd returns. This
        # makes a surviving child observable without process-state/zombie races.
        # It also self-exits after five seconds; cleanup targets only this fixture.
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            ready, release, leaked = [root / name for name in ('ready', 'release', 'leaked')]
            script = root / 'child.py'
            script.write_text(
                'import os, pathlib, time\n'
                f'ready = pathlib.Path({str(ready)!r})\n'
                f'release = pathlib.Path({str(release)!r})\n'
                f'leaked = pathlib.Path({str(leaked)!r})\n'
                'ready.write_text(str(os.getpid()))\n'
                'deadline = time.monotonic() + 5\n'
                'while time.monotonic() < deadline:\n'
                '    if release.exists():\n'
                '        leaked.write_text("ran after timeout")\n'
                '        break\n'
                '    time.sleep(0.01)\n', encoding='utf-8')
            # Retain the shell in one case, exit it in the other. In the latter
            # case the child still holds the stdout pipe after the leader exits.
            command = shlex.join([sys.executable, str(script)]) + ' & '
            command += 'wait' if wait_for_child else 'exit 0'
            own_group = os.getpgrp()
            try:
                code, _, output = runner.run_cmd(command, 1)
                self.assertEqual(code, 124, output)
                self.assertIn('TIMEOUT', output)
                self.assertTrue(ready.exists(), 'fixture did not start before the timeout')
                self.assertEqual(os.getpgrp(), own_group, 'caller group must not change')
                release.touch()
                deadline = time.monotonic() + 0.5
                while time.monotonic() < deadline and not leaked.exists():
                    time.sleep(0.01)
                self.assertFalse(leaked.exists(), 'child kept working after run_cmd timed out')
            finally:
                # On the unfixed source the failure is intentional. Do not leave
                # its child running, even when an earlier assertion fails.
                if ready.exists():
                    try:
                        os.kill(int(ready.read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_timeout_stops_child_of_waiting_shell(self):
        self.assert_descendant_stops(wait_for_child=True)

    def test_timeout_stops_child_after_shell_already_exited(self):
        self.assert_descendant_stops(wait_for_child=False)

    def test_timeout_does_not_kill_an_unrelated_process(self):
        with subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(10)'],
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as sibling:
            try:
                code, _, _ = runner.run_cmd(
                    [sys.executable, '-c', 'import time; time.sleep(10)'], 0.1)
                self.assertEqual(code, 124)
                self.assertIsNone(sibling.poll())
            finally:
                sibling.kill()
                sibling.wait()

    def test_shell_success_is_unchanged(self):
        code, _, output = runner.run_cmd('printf "shell-ok"', 5)
        self.assertEqual((code, output), (0, 'shell-ok'))


if __name__ == '__main__':
    unittest.main()
