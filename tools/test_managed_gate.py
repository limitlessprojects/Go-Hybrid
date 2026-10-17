"""Regression tests for lossless source/report pairing in the managed gate."""
import contextlib
import importlib.util
import io
import json
import pathlib
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'managed_gate', pathlib.Path(__file__).with_name('managed_gate.py'))
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)


def block(reason):
    return {'reason': reason, 'category': 'other'}


class ManagedGateTests(unittest.TestCase):
    def audit(self, source, blocks, returncode=0):
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text(source, encoding='utf-8')
            report = {'summary': {'managed': {
                'managed_blocks': len(blocks), 'blocks': blocks}}}
            result = subprocess.CompletedProcess([], returncode, json.dumps(report), '')
            with patch.object(gate.subprocess, 'run', return_value=result):
                return gate.audit('hyb', str(prog))


    def test_separate_lines_keep_existing_baseline_keys(self):
        found = self.audit('managed("a", "other") {}\nmanaged("b", "other") {}',
                           [block('a'), block('b')])
        self.assertEqual([key.rsplit(':', 1)[1] for key in found], ['1', '2'])
        self.assertEqual(list(found.values()), [block('a'), block('b')])

    def test_empty_file_has_no_blocks(self):
        self.assertEqual(self.audit('fn main() {}', []), {})

    def test_removed_block_keeps_remaining_location(self):
        found = self.audit('\nmanaged("b", "other") {}', [block('b')])
        self.assertEqual([key.rsplit(':', 1)[1] for key in found], ['2'])

    def assert_mapping_error(self, source, blocks):
        output = io.StringIO()
        with contextlib.redirect_stdout(output), self.assertRaises(SystemExit) as error:
            self.audit(source, blocks)
        self.assertEqual(error.exception.code, 2)
        self.assertIn('GATE ERROR', output.getvalue())

    def test_two_blocks_on_one_line_cannot_create_ambiguous_baseline(self):
        self.assert_mapping_error('managed("a", "other") {} managed("c", "other") {}',
                                  [block('a'), block('c')])

    def test_inserted_middle_block_cannot_be_overwritten_at_question_mark_key(self):
        self.assert_mapping_error(
            'managed("a", "other") {} managed("NEW", "other") {} managed("c", "other") {}',
            [block('a'), block('NEW'), block('c')])

    def test_unlocated_reported_block_fails_closed(self):
        self.assert_mapping_error('fn main() {}', [block('missing')])

    def test_extra_source_candidate_is_not_paired_with_wrong_report(self):
        self.assert_mapping_error('managed("extra", "other") {}\nmanaged("a", "other") {}',
                                  [block('a')])

    def test_comments_and_strings_do_not_spoof_source_line(self):
        # Issue #119: comments and string literals mentioning managed(...) must
        # be ignored by the lexical scan, and whitespace between `managed` and
        # `(` must still match the real statement line.
        src = '\n'.join([
            'fn main() {',
            '  // managed("not executable", "other")',
            '  /* managed("block comment", "other") */',
            '  s := "managed(\\"str\\", \\"other\\")"',
            '  r := `managed("raw", "other")`',
            '  managed ("reviewed", "other") {}',
            '}',
        ])
        found = self.audit(src, [block('reviewed')])
        self.assertEqual([key.rsplit(':', 1)[1] for key in found], ['6'])

    def test_moving_block_below_comment_changes_baseline_key(self):
        # Issue #119 reproducer: moving a spaced `managed (...)` block while a
        # preceding comment mentions `managed(` must change the recorded line.
        before = '\n'.join([
            'fn main() {',
            '  // managed("not executable", "other")',
            '  managed ("reviewed", "other") {}',
            '}',
        ])
        after = '\n'.join([
            'fn main() {',
            '  // managed("not executable", "other")',
            '  x := 1',
            '  y := 2',
            '  managed ("reviewed", "other") {}',
            '}',
        ])
        found_before = self.audit(before, [block('reviewed')])
        found_after = self.audit(after, [block('reviewed')])
        self.assertEqual([key.rsplit(':', 1)[1] for key in found_before], ['3'])
        self.assertEqual([key.rsplit(':', 1)[1] for key in found_after], ['5'])

    def test_invalid_category_still_fails_closed(self):
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as error:
            self.audit('managed("a", "invalid") {}',
                       [{'reason': 'a', 'category': 'invalid'}])
        self.assertEqual(error.exception.code, 2)

    def test_checker_failure_fails_the_gate_even_with_parseable_output(self):
        # Issue #53: a rejected/malformed program must not read as "zero
        # blocks". Even a parseable JSON report cannot back a PASS (or a
        # rebaseline) when hyb check itself exited nonzero.
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as error:
            self.audit('fn main( {}', [], returncode=1)
        self.assertEqual(error.exception.code, 2)

    def test_malformed_report_cannot_pass_or_replace_baseline(self):
        payloads = [
            {}, {'summary': None}, {'summary': {}},
            {'summary': {'managed': None}},
            {'summary': {'managed': {}}},
            {'summary': {'managed': {'blocks': {}}}},
            {'summary': {'managed': {'blocks': ''}}},
            [], None, {'summary': []}, {'summary': {'managed': []}},
            {'summary': {'managed': {'blocks': None}}},
            {'summary': {'managed': {'blocks': [None]}}},
            {'summary': {'managed': {'blocks': [{'category': []}]}}},
            {'summary': {'managed': {'managed_blocks': 1}}},
            {'summary': {'managed': {'managed_blocks': True}}},
            {'summary': {'managed': {'managed_blocks': 0, 'blocks': {}}}},
            {'summary': {'managed': {'managed_blocks': 0, 'blocks': ''}}},
            {'summary': {'managed': {'managed_blocks': 1, 'blocks': [None]}}},
            {'summary': {'managed': {'managed_blocks': 1,
                                      'blocks': [{'category': []}]}}},
        ]
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            original = json.dumps({'version': 1, 'blocks': {
                f'{prog}:1': block('previously reviewed')}}).encode()
            for payload in payloads:
                for rebaseline in (False, True):
                    with self.subTest(payload=payload, rebaseline=rebaseline):
                        baseline.write_bytes(original)
                        argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
                        argv += ['hyb', str(baseline), str(prog)]
                        result = subprocess.CompletedProcess([], 0, json.dumps(payload), '')
                        output = io.StringIO()
                        with patch.object(gate.sys, 'argv', argv), \
                             patch.object(gate.subprocess, 'run', return_value=result), \
                             contextlib.redirect_stdout(output):
                            with self.assertRaises(SystemExit) as error:
                                gate.main()
                        self.assertEqual(error.exception.code, 2)
                        self.assertIn('GATE ERROR', output.getvalue())
                        self.assertNotIn('managed gate: PASS', output.getvalue())
                        self.assertEqual(baseline.read_bytes(), original)

    def test_explicit_empty_report_can_remove_last_reviewed_block(self):
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            original = json.dumps({'version': 1, 'blocks': {
                f'{prog}:1': block('previously reviewed')}}).encode()
            # Rust omits blocks for an empty Vec; the counter proves this is
            # an actual empty report rather than absent audit data.
            report = {'summary': {'managed': {'managed_blocks': 0}}}
            result = subprocess.CompletedProcess([], 0, json.dumps(report), '')
            for rebaseline in (False, True):
                with self.subTest(rebaseline=rebaseline):
                    baseline.write_bytes(original)
                    argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
                    argv += ['hyb', str(baseline), str(prog)]
                    with patch.object(gate.sys, 'argv', argv), \
                         patch.object(gate.subprocess, 'run', return_value=result), \
                         contextlib.redirect_stdout(io.StringIO()):
                        gate.main()
                    if rebaseline:
                        self.assertEqual(json.loads(baseline.read_bytes())['blocks'], {})
                    else:
                        self.assertEqual(baseline.read_bytes(), original)

    def assert_invalid_baseline(self, original, diagnostic):
        # A bad inventory must fail before scanning or rewriting it in either
        # mode, even when the supplied source and checker are otherwise valid.
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            for rebaseline in (False, True):
                with self.subTest(rebaseline=rebaseline):
                    baseline.write_bytes(original)
                    argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
                    argv += ['hyb', str(baseline), str(prog)]
                    output = io.StringIO()
                    with patch.object(gate.sys, 'argv', argv), \
                         patch.object(gate, 'audit') as audit, \
                         contextlib.redirect_stdout(output):
                        with self.assertRaises(SystemExit) as error:
                            gate.main()
                    self.assertEqual(error.exception.code, 2)
                    self.assertIn(diagnostic, output.getvalue())
                    self.assertNotIn('managed gate: PASS', output.getvalue())
                    audit.assert_not_called()
                    self.assertEqual(baseline.read_bytes(), original)

    def test_malformed_baseline_json_fails_closed(self):
        for original in (b'', b'{not json}', b'{"version": 1, ', b'\xff'):
            with self.subTest(original=original):
                self.assert_invalid_baseline(original, 'malformed baseline JSON')

    def test_non_object_baseline_fails_closed(self):
        for value in ([], 'string', 123, None, True):
            with self.subTest(value=value):
                self.assert_invalid_baseline(
                    json.dumps(value).encode(), 'baseline must be a JSON object')

    def test_non_object_baseline_blocks_fails_closed(self):
        for value in ([], 'invalid', 42, None, True):
            with self.subTest(value=value):
                self.assert_invalid_baseline(
                    json.dumps({'version': 1, 'blocks': value}).encode(),
                    'baseline blocks must be a JSON object')

    def test_missing_baseline_and_legacy_object_allow_real_empty_scan(self):
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            for original in (None, b'{"version": 1}'):
                for rebaseline in (False, True):
                    with self.subTest(original=original, rebaseline=rebaseline):
                        baseline.unlink(missing_ok=True)
                        if original is not None:
                            baseline.write_bytes(original)
                        argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
                        argv += ['hyb', str(baseline), str(prog)]
                        output = io.StringIO()
                        with patch.object(gate.sys, 'argv', argv), \
                             patch.object(gate, 'audit', return_value={}) as audit, \
                             contextlib.redirect_stdout(output):
                            gate.main()
                        audit.assert_called_once_with('hyb', str(prog))
                        if rebaseline:
                            self.assertEqual(json.loads(baseline.read_bytes()),
                                             {'version': 1, 'blocks': {}})
                            self.assertTrue(baseline.read_bytes().endswith(b'\n'))
                        elif original is None:
                            self.assertFalse(baseline.exists())
                        else:
                            self.assertEqual(baseline.read_bytes(), original)
                        self.assertIn('rebaseline:' if rebaseline else 'managed gate: PASS',
                                      output.getvalue())


if __name__ == '__main__':
    unittest.main()
