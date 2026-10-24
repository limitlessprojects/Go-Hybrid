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


class RunnerRedactionTests(unittest.TestCase):
    def test_secret_markers_and_credential_values_are_redacted(self):
        secret_keys = {
            key: 'fake-value' for key in (
                'PAT', 'GITHUB_PAT', 'GH-PAT', 'API_KEY', 'CLIENT_KEY',
                'TOKEN', 'SECRET', 'PASSWORD', 'CREDENTIAL', 'CLIENTCREDENTIAL',
            )
        }
        credential_values = {
            'value-{}'.format(idx): 'prefix-{}example'.format(prefix)
            for idx, prefix in enumerate(
                ('ghp_', 'gho_', 'ghs_', 'ghr_', 'github_pat_', 'KGAT_'))
        }
        redacted = runner.redact({
            **secret_keys, **credential_values, 'EMPTY_PAT': '',
        })

        for key, value in secret_keys.items():
            self.assertEqual(redacted[key], '[REDACTED len={}]'.format(len(value)))
        for key, value in credential_values.items():
            self.assertEqual(redacted[key], '[REDACTED len={}]'.format(len(value)))
        self.assertEqual(redacted['EMPTY_PAT'], '[EMPTY]')

    def test_key_segments_with_trailing_qualifiers_are_redacted(self):
        secret_keys = {
            key: 'unprefixed-secret-value' for key in (
                'CI_API_KEY_BASE64', 'GITHUB_SSH_KEY_B64', 'MY_KEY_FILE',
            )
        }

        redacted = runner.redact(secret_keys)

        for key, value in secret_keys.items():
            self.assertEqual(redacted[key], '[REDACTED len={}]'.format(len(value)))

    def test_plural_secret_segments_with_trailing_qualifiers_are_redacted(self):
        secret_keys = {
            key: 'marker-value' for key in (
                'CI_TOKENS_JSON', 'GITHUB_SECRETS_B64', 'KAGGLE_PASSWORDS_JSON',
                'CI_CREDENTIALS_JSON', 'GITHUB_PATS_B64', 'MY_KEYS_FILE',
                'CI_API_KEYS_BASE64',
            )
        }

        redacted = runner.redact(secret_keys)

        for key, value in secret_keys.items():
            self.assertEqual(redacted[key], '[REDACTED len={}]'.format(len(value)))

    def test_plural_compact_secret_suffixes_are_redacted(self):
        secret_keys = {
            key: 'marker-value' for key in (
                'CLIENTCREDENTIALS', 'XTOKENS', 'GITHUBPATS',
            )
        }

        redacted = runner.redact(secret_keys)

        for key, value in secret_keys.items():
            self.assertEqual(redacted[key], '[REDACTED len={}]'.format(len(value)))

    def test_path_and_nonsecret_names_remain_unredacted(self):
        paths = {
            'GITHUB_PATH': '/tmp/github-path',
            'GITHUB_EVENT_PATH': '/tmp/event.json',
            'LD_LIBRARY_PATH': '/opt/lib',
            'PYTHONPATH': '/opt/python',
            'GOPATH': '/home/runner/go',
            'KAGGLE_DATASET_PATH': '/input/data',
            'GITHUB_DISPATCH_TYPE': 'workflow_dispatch',
            'COMPATIBILITY': 'preserve-me',
            'PATHS': '/tmp/paths',
            'KEYSTONE': 'stone',
        }

        self.assertEqual(runner.redact(paths), paths)


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

    def test_malformed_setup_step_continues_through_setup_and_commands(self):
        code, summary = self.run_job(
            [step('before', 'pass'), 'malformed', step('after', 'pass')],
            [step('command', 'pass')])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [
            ('before', 0), ('setup-1', 1), ('after', 0), ('command', 0),
        ])
        self.assertIn('must be an object, got str', summary['steps'][1]['out_tail'])

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

    def test_malformed_command_step_continues_through_later_commands(self):
        code, summary = self.run_job(
            [], [step('before', 'pass'), None, step('after', 'pass')])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual([(s['name'], s['exit']) for s in summary['steps']], [
            ('before', 0), ('cmd-1', 1), ('after', 0),
        ])
        self.assertIn('must be an object, got NoneType', summary['steps'][1]['out_tail'])

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

    def test_empty_job_writes_summary_and_fails(self):
        code, summary = self.run_job([], [])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual(summary['steps'][0]['name'], 'job-spec')
        self.assertEqual(summary['steps'][0]['exit'], 1)

    def test_missing_job_file_writes_summary_and_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            results = root / 'results'
            argv = ['runner.py', str(root / 'missing.json'), '--results-dir', str(results),
                    '--repo-dir', str(root / 'no-checkout')]
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            self.assertEqual(stopped.exception.code, 1)
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            self.assertFalse(summary['ok'])
            self.assertEqual(summary['steps'][0]['name'], 'job-load')
            self.assertEqual(summary['steps'][0]['exit'], 1)

    def test_malformed_json_job_writes_summary_and_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            job_path = root / 'job.json'
            job_path.write_text('{invalid-json', encoding='utf-8')
            results = root / 'results'
            argv = ['runner.py', str(job_path), '--results-dir', str(results),
                    '--repo-dir', str(root / 'no-checkout')]
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            self.assertEqual(stopped.exception.code, 1)
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            self.assertFalse(summary['ok'])
            self.assertEqual(summary['steps'][0]['name'], 'job-load')
            self.assertEqual(summary['steps'][0]['exit'], 1)

    def test_non_dict_job_writes_summary_and_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            job_path = root / 'job.json'
            job_path.write_text('[1, 2, 3]', encoding='utf-8')
            results = root / 'results'
            argv = ['runner.py', str(job_path), '--results-dir', str(results),
                    '--repo-dir', str(root / 'no-checkout')]
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            self.assertEqual(stopped.exception.code, 1)
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            self.assertFalse(summary['ok'])
            self.assertEqual(summary['steps'][0]['name'], 'job-spec')
            self.assertEqual(summary['steps'][0]['exit'], 1)

    def test_step_missing_run_command_fails(self):
        code, summary = self.run_job([], [{'name': 'missing-run'}])
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertEqual(summary['steps'][0]['name'], 'missing-run')
        self.assertEqual(summary['steps'][0]['exit'], 1)

    def test_invalid_run_types_write_failed_summary(self):
        for command in (42, {'command': 'true'}, [], [42], False, None):
            with self.subTest(command=command):
                code, summary = self.run_job([], [{'name': 'invalid-run', 'run': command}])
                self.assertEqual(code, 1)
                self.assertFalse(summary['ok'])
                self.assertEqual([(s['name'], s['exit']) for s in summary['steps']],
                                 [('invalid-run', 125)])
                self.assertIn('runner error', summary['steps'][0]['out_tail'])
                self.assertIsNotNone(summary['finished'])

    def test_run_cmd_rejects_bytes_argv_before_subprocess(self):
        command = [os.fsencode(sys.executable), b'-c', b'print("must not run")']
        with patch.object(runner.subprocess, 'run') as subprocess_run:
            code, _, output = runner.run_cmd(command, 1)
        self.assertEqual(code, 125)
        self.assertIn('<<runner error:', output)
        subprocess_run.assert_not_called()

    def test_invalid_timeout_types_write_failed_summary(self):
        timeouts = (
            ('invalid', 'invalid'), ('mapping', {'seconds': 1}),
            ('true', True), ('false', False), ('zero', 0), ('negative', -1),
            ('nan', float('nan')), ('positive-infinity', float('inf')),
            ('negative-infinity', -float('inf')), ('none', None),
        )
        for label, timeout in timeouts:
            with self.subTest(timeout=label):
                with patch.object(runner.subprocess, 'run') as subprocess_run:
                    code, summary = self.run_job(
                        [], [step('invalid-timeout', 'pass', timeout=timeout)])
                self.assertEqual(code, 1)
                self.assertFalse(summary['ok'])
                self.assertEqual([(s['name'], s['exit']) for s in summary['steps']],
                                 [('invalid-timeout', 125)])
                self.assertIn('<<runner error:', summary['steps'][0]['out_tail'])
                self.assertIsNotNone(summary['finished'])
                subprocess_run.assert_not_called()

    def test_invalid_commands_type_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            job_path = root / 'job.json'
            job_path.write_text(json.dumps({'job': 'test', 'commands': 'not-a-list'}), encoding='utf-8')
            results = root / 'results'
            argv = ['runner.py', str(job_path), '--results-dir', str(results),
                    '--repo-dir', str(root / 'no-checkout')]
            with patch.object(sys, 'argv', argv), patch.dict(os.environ), \
                    patch.object(runner, 'env_report', return_value={'test': True}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with self.assertRaises(SystemExit) as stopped:
                    runner.main()
            self.assertEqual(stopped.exception.code, 1)
            summary = json.loads((results / 'summary.json').read_text(encoding='utf-8'))
            self.assertFalse(summary['ok'])
            self.assertEqual(summary['steps'][0]['name'], 'job-spec')
            self.assertEqual(summary['steps'][0]['exit'], 1)

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
            if pin == 'exact':
                sha_pin = at
            elif pin == 'uppercase-padded':
                sha_pin = '  \t{}\n '.format(at.upper())
            else:
                sha_pin = pin
            setup_marker, command_marker = root / 'setup.ran', root / 'command.ran'
            job_path, results = root / 'job.json', root / 'results'
            job_path.write_text(json.dumps({
                'job': 'worktree-pin', 'sha': sha_pin,
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

    def test_uppercase_whitespace_padded_sha_matches(self):
        code, summary, setup_ran, command_ran = self.run_worktree_job('uppercase-padded')
        self.assertEqual(code, 0)
        self.assertTrue(summary['ok'])
        self.assertTrue(setup_ran)
        self.assertTrue(command_ran)

    def test_non_string_sha_pins_fail_closed(self):
        for pin in (12345, True, ['HEAD'], None):
            with self.subTest(pin=pin):
                code, summary, setup_ran, command_ran = self.run_worktree_job(pin)
                self.assertEqual(code, 1)
                self.assertFalse(summary['ok'])
                self.assertFalse(setup_ran)
                self.assertFalse(command_ran)
                self.assertEqual([(s['name'], s['exit']) for s in summary['steps']],
                                 [('sha-check', 1)])

    def test_broken_git_file_fails_closed(self):
        code, summary, setup_ran, command_ran = self.run_worktree_job('HEAD', broken=True)
        self.assertEqual(code, 1)
        self.assertFalse(summary['ok'])
        self.assertFalse(setup_ran)
        self.assertFalse(command_ran)
        self.assertIn('failed to resolve HEAD', summary['steps'][0]['out_tail'])


if __name__ == '__main__':
    unittest.main()
