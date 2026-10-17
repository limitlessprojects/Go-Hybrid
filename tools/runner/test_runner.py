"""Verify hard/soft job failures with real, harmless subprocess commands."""
import contextlib
import importlib.util
import io
import json
import os
import pathlib
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


class RunnerOutputTests(unittest.TestCase):
    def timed(self, code):
        return runner.run_cmd([sys.executable, '-u', '-c', code], 0.5)

    def test_timeout_keeps_stdout_and_stderr(self):
        code, _, tail = self.timed(
            'import sys,time; print("compile-start", flush=True); '
            'print("specific-error", file=sys.stderr, flush=True); time.sleep(60)')
        self.assertEqual(code, 124)
        self.assertIn('compile-start', tail)
        self.assertIn('specific-error', tail)
        self.assertIn('<<TIMEOUT after 0.5s>>', tail)

    def test_timeout_decodes_incomplete_utf8(self):
        code, _, tail = self.timed(
            r"import os,time; os.write(1,b'partial-\xe2\x82'); time.sleep(60)")
        self.assertEqual(code, 124)
        self.assertIn('partial-\ufffd', tail)
        self.assertIn('<<TIMEOUT', tail)

    def test_timeout_output_retains_tail_and_bound(self):
        code, _, tail = self.timed(
            'import sys,time; print("discarded-prefix"+"x"*6000+"last-diagnostic", '
            'flush=True); time.sleep(60)')
        self.assertEqual(code, 124)
        self.assertLessEqual(len(tail), 4000)
        self.assertIn('last-diagnostic', tail)
        self.assertNotIn('discarded-prefix', tail)
        self.assertIn('<<TIMEOUT', tail)

    def test_silent_timeout_keeps_existing_marker(self):
        code, _, tail = self.timed('import time; time.sleep(60)')
        self.assertEqual(code, 124)
        self.assertEqual(tail, '<<TIMEOUT after 0.5s>>')

    def test_completed_output_and_exit_remain_unchanged(self):
        for expected in [0, 7]:
            with self.subTest(exit=expected):
                code, _, tail = runner.run_cmd(
                    [sys.executable, '-c', 'print("completed"); raise SystemExit(%d)' % expected], 10)
                self.assertEqual(code, expected)
                self.assertEqual(tail, 'completed\n')

    def test_job_summary_keeps_timeout_diagnostic(self):
        code, summary = RunnerFailureTests.run_job(self, [], [step(
            'compile', 'import sys,time; print("compile-progress", flush=True); '
            'print("compiler-context", file=sys.stderr, flush=True); time.sleep(60)',
            True, 0.5)])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual(summary['steps'][0]['exit'], 124)
        self.assertIn('compiler-context', summary['steps'][0]['out_tail'])
        self.assertIn('<<TIMEOUT', summary['steps'][0]['out_tail'])


if __name__ == '__main__':
    unittest.main()
