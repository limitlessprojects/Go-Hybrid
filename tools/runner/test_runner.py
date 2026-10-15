"""Verify hard/soft job failures with real, harmless subprocess commands."""
import contextlib
import importlib.util
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
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


class WorktreeShaTests(unittest.TestCase):
    def run_worktree_job(self, pin, broken=False):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            repo, worktree = root / 'repo', root / 'worktree'
            subprocess.run(['git', 'init', '-q', str(repo)], check=True)
            subprocess.run(['git', '-C', str(repo), '-c', 'user.name=Fixture',
                            '-c', 'user.email=fixture@example.invalid',
                            '-c', 'commit.gpgsign=false', 'commit', '-q',
                            '--allow-empty', '-m', 'fixture'], check=True)
            subprocess.run(['git', '-C', str(repo), 'worktree', 'add',
                            '--quiet', '--detach', str(worktree), 'HEAD'], check=True)
            at = subprocess.check_output(['git', '-C', str(worktree),
                                          'rev-parse', 'HEAD'], text=True).strip()
            self.assertTrue((worktree / '.git').is_file())
            if broken:
                (worktree / '.git').write_text('gitdir: missing-metadata\n', encoding='utf-8')
            setup_marker, command_marker = root / 'setup.ran', root / 'command.ran'
            job_path, results = root / 'job.json', root / 'results'
            job_path.write_text(json.dumps({
                'job': 'worktree-pin', 'sha': at if pin == 'exact' else pin,
                'setup': [step('setup', f'from pathlib import Path; Path({str(setup_marker)!r}).touch()')],
                'commands': [step('command', f'from pathlib import Path; Path({str(command_marker)!r}).touch()')],
            }), encoding='utf-8')
            argv = ['runner.py', str(job_path), '--results-dir', str(results),
                    '--repo-dir', str(worktree)]
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            return stopped.exception.code, summary, setup_marker.exists(), command_marker.exists()

    def test_wrong_worktree_sha_skips_setup_and_commands(self):
        code, summary, setup_ran, command_ran = self.run_worktree_job('0' * 40)
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertFalse(setup_ran)
        self.assertFalse(command_ran)
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [('sha-check', 1)])

    def test_matching_worktree_sha_and_head_run_both_phases(self):
        for pin in ('exact', 'HEAD'):
            with self.subTest(pin=pin):
                code, summary, setup_ran, command_ran = self.run_worktree_job(pin)
                self.assertEqual(code, 0)
                self.assertTrue(summary['ok'])
                self.assertTrue(setup_ran)
                self.assertTrue(command_ran)
                self.assertEqual([s['name'] for s in summary['steps']], ['sha-check', 'setup', 'command'])

    def test_broken_git_file_fails_closed(self):
        code, summary, setup_ran, command_ran = self.run_worktree_job('HEAD', broken=True)
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertFalse(setup_ran)
        self.assertFalse(command_ran)
        self.assertIn('failed to resolve HEAD', summary['steps'][0]['out_tail'])


if __name__ == '__main__':
    unittest.main()
