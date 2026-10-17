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
    def run_job(self, setup, commands, expected_results=None):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            job_path = root / 'job.json'
            results = root / 'results'
            job_path.write_text(json.dumps({'job': 'test', 'setup': setup,
                                          'commands': commands,
                                          'expected_results': [] if expected_results is None else expected_results}), encoding='utf-8')
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

    def test_successful_command_cannot_hide_missing_result(self):
        code, summary = self.run_job([], [step('noop', 'pass')],
                                     ['results/measurement.json', 'results/summary.json'])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual(summary['steps'][-1]['name'], 'expected-result')
        self.assertIn('results/measurement.json', summary['steps'][-1]['out_tail'])

    def test_results_prefix_uses_selected_output_directory(self):
        code, summary = self.run_job([], [step('write',
            "import os,pathlib; p=pathlib.Path(os.environ['RESULTS_DIR'])/'nested'; "
            "p.mkdir(); (p/'measurement.json').write_text('{}')")],
            ['results/nested/measurement.json', 'results/summary.json'])
        self.assertEqual(code, 0)
        self.assertTrue(summary['ok'])

    def test_directory_does_not_satisfy_expected_file(self):
        code, summary = self.run_job([], [step('mkdir',
            "import os,pathlib; (pathlib.Path(os.environ['RESULTS_DIR'])/'measurement').mkdir()")],
            ['results/measurement'])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])

    def test_generated_summary_is_a_valid_expected_result(self):
        code, summary = self.run_job([], [step('noop', 'pass')], ['results/summary.json'])
        self.assertEqual(code, 0)
        self.assertTrue(summary['ok'])

    def test_invalid_output_declaration_records_failure_in_summary(self):
        for expected in ('results/missing.json', [None], ['']):
            with self.subTest(expected=expected):
                code, summary = self.run_job([], [step('noop', 'pass')], expected)
                self.assertEqual(code, 1)
                self.assertFalse(summary['ok'])
                self.assertEqual(summary['steps'][-1]['name'], 'expected-result')


if __name__ == '__main__':
    unittest.main()
