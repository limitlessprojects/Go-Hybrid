"""Regression tests for lossless source/report pairing in the managed gate."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import pathlib
import subprocess
import sys
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
    def test_missing_source_arguments_fail_without_changing_baseline(self):
        script = pathlib.Path(__file__).with_name('managed_gate.py')
        for rebaseline in (False, True):
            for existing in (False, True):
                with self.subTest(rebaseline=rebaseline, existing=existing), \
                        tempfile.TemporaryDirectory() as directory:
                    baseline = pathlib.Path(directory) / 'baseline.json'
                    original = b'{"version": 1, "blocks": {"reviewed.hy:1": {"reason": "reviewed", "category": "other"}}}\n'
                    if existing:
                        baseline.write_bytes(original)
                    args = ['--rebaseline'] if rebaseline else []
                    result = subprocess.run(
                        [sys.executable, str(script), *args, 'unused-hyb', str(baseline)],
                        capture_output=True, text=True, timeout=10,
                    )
                    self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                    self.assertNotIn('managed gate: PASS', result.stdout)
                    self.assertIn('source', result.stderr)
                    if existing:
                        self.assertEqual(baseline.read_bytes(), original)
                    else:
                        self.assertFalse(baseline.exists())

    def test_explicit_source_without_managed_blocks_can_clear_baseline(self):
        # A real scan yielding zero blocks must stay legal: this represents
        # removal of the last hatch, unlike omitting the scan inputs entirely.
        for rebaseline in (False, True):
            with self.subTest(rebaseline=rebaseline), tempfile.TemporaryDirectory() as directory:
                root = pathlib.Path(directory)
                source = root / 'plain.hy'
                source.write_text('fn main() {}', encoding='utf-8')
                baseline = root / 'baseline.json'
                # The reviewed block belongs to the scanned program, so an
                # explicit zero-block scan may clear it (pr-434 scope rule:
                # unaudited programs' blocks would be preserved instead).
                original = {'version': 1, 'blocks': {f'{source}:1': block('reviewed')}}
                baseline.write_text(json.dumps(original), encoding='utf-8')
                args = ['managed_gate.py']
                if rebaseline:
                    args.append('--rebaseline')
                args.extend(['hyb', str(baseline), str(source)])
                # Current audit() contract: the managed report carries the
                # reported count and the block list must be consistent with it.
                report = subprocess.CompletedProcess(
                    [], 0,
                    json.dumps({'summary': {'managed': {'managed_blocks': 0, 'blocks': []}}}).encode('utf-8'),
                    b'')
                with patch.object(sys, 'argv', args), \
                        patch.object(gate.subprocess, 'run', return_value=report) as checker, \
                        contextlib.redirect_stdout(io.StringIO()) as output:
                    gate.main()
                self.assertEqual(checker.call_count, 1)
                expected = {'version': 1, 'blocks': {}} if rebaseline else original
                self.assertEqual(json.loads(baseline.read_text()), expected)
                self.assertIn('rebaseline:' if rebaseline else 'managed gate: PASS', output.getvalue())

    def audit(self, source, blocks, returncode=0):
        with tempfile.TemporaryDirectory() as directory:
            prog = pathlib.Path(directory) / 'test.hy'
            prog.write_text(source, encoding='utf-8')
            report = {'summary': {'managed': {
                'managed_blocks': len(blocks), 'blocks': blocks}}}
            result = subprocess.CompletedProcess([], returncode, json.dumps(report).encode('utf-8'), b'')
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
                        result = subprocess.CompletedProcess([], 0, json.dumps(payload).encode('utf-8'), b'')
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
            result = subprocess.CompletedProcess([], 0, json.dumps(report).encode('utf-8'), b'')
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


class AtomicBaselineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.baseline = self.root / "baseline.json"
        # The stale reviewed block belongs to the audited program (new.hy),
        # so the rebaseline below is a full replacement under the pr-434
        # scope rule (unaudited programs' blocks are preserved instead).
        self.original = b'{"version": 1, "blocks": {"new.hy:0": {"reason": "reviewed"}}}\n'
        self.baseline.write_bytes(self.original)
        self.baseline.chmod(0o640)
        self.current = {"new.hy:1": {"reason": "new", "category": "other"}}

    def rebaseline(self):
        with patch.object(gate.sys, "argv", ["managed_gate.py", "--rebaseline",
                          "hyb", str(self.baseline), "new.hy"]), \
             patch.object(gate, "audit", return_value=self.current), \
             contextlib.redirect_stdout(io.StringIO()):
            gate.main()

    def test_success_replaces_complete_json_and_preserves_mode(self):
        self.rebaseline()
        self.assertEqual(json.loads(self.baseline.read_text())["blocks"], self.current)
        self.assertEqual(self.baseline.stat().st_mode & 0o777, 0o640)
        self.assertEqual(list(self.root.iterdir()), [self.baseline])

    def test_replace_failure_preserves_original_and_removes_temporary_file(self):
        with patch.object(os, "replace", side_effect=OSError("synthetic replace failure")):
            with self.assertRaises(SystemExit) as error:
                self.rebaseline()
        self.assertEqual(error.exception.code, 2)
        self.assertEqual(self.baseline.read_bytes(), self.original)
        self.assertEqual(list(self.root.iterdir()), [self.baseline])

    def test_flush_failure_preserves_original_and_removes_temporary_file(self):
        with patch.object(os, "fsync", side_effect=OSError("synthetic flush failure")):
            with self.assertRaises(SystemExit) as error:
                self.rebaseline()
        self.assertEqual(error.exception.code, 2)
        self.assertEqual(self.baseline.read_bytes(), self.original)
        self.assertEqual(list(self.root.iterdir()), [self.baseline])

    @unittest.skipUnless(os.name == "posix", "requires symlinks")
    def test_symlink_keeps_pointing_at_updated_target(self):
        target = self.baseline
        link = self.root / "baseline-link.json"
        link.symlink_to(target.name)
        self.baseline = link
        self.rebaseline()
        self.assertTrue(link.is_symlink())
        self.assertEqual(json.loads(target.read_text())["blocks"], self.current)

    @unittest.skipUnless(os.name == "posix", "requires the POSIX file-size limit")
    def test_real_write_failure_preserves_reviewed_baseline(self):
        # This is a real filesystem error, not a mocked dump/write: the child
        # cannot write a file larger than 128 bytes. An in-place JSON dump
        # truncates the old baseline before buffered close raises EFBIG.
        script = r'''
import importlib.util, pathlib, resource, signal, sys
spec = importlib.util.spec_from_file_location("managed_gate", sys.argv[1])
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)
gate.audit = lambda *_: {"new.hy:1": {"reason": "x" * 1000, "category": "other"}}
sys.argv = ["managed_gate.py", "--rebaseline", "hyb", sys.argv[2], "new.hy"]
signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
resource.setrlimit(resource.RLIMIT_FSIZE, (128, 128))
gate.main()
'''
        result = subprocess.run(
            [sys.executable, "-B", "-c", script, str(Path(gate.__file__)), str(self.baseline)],
            capture_output=True, text=True, timeout=10)
        self.assertEqual(self.baseline.read_bytes(), self.original)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("GATE ERROR", result.stdout)
        self.assertNotIn("rebaseline:", result.stdout)
        self.assertEqual(list(self.root.iterdir()), [self.baseline])



    def test_zero_programs_fails_closed_without_modifying_baseline(self):
        # pr-434: zero program arguments (e.g. an unmatched shell glob) must
        # not pass the gate or touch the baseline, in either mode. The guard
        # landed with pr-113 reports on stderr; this pins the fail-closed
        # contract end to end.
        with tempfile.TemporaryDirectory() as directory:
            baseline = pathlib.Path(directory) / 'baseline.json'
            original = json.dumps({'version': 1, 'blocks': {
                'unrelated.hy:10': block('reviewed')}}).encode()
            for rebaseline in (False, True):
                with self.subTest(rebaseline=rebaseline):
                    baseline.write_bytes(original)
                    argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
                    argv += ['hyb', str(baseline)]
                    output = io.StringIO()
                    errors = io.StringIO()
                    with patch.object(gate.sys, 'argv', argv), \
                         contextlib.redirect_stdout(output), \
                         contextlib.redirect_stderr(errors):
                        with self.assertRaises(SystemExit) as error:
                            gate.main()
                    self.assertEqual(error.exception.code, 2)
                    self.assertIn('GATE ERROR', output.getvalue() + errors.getvalue())
                    self.assertIn('at least one source file', errors.getvalue())
                    self.assertNotIn('managed gate: PASS', output.getvalue())
                    self.assertNotIn('rebaseline:', output.getvalue())
                    self.assertEqual(baseline.read_bytes(), original)

    def test_partial_program_audit_preserves_unrelated_baseline_blocks(self):
        # pr-434: auditing a subset must not report unrelated baseline blocks
        # as removed, and must not wipe them in --rebaseline.
        with tempfile.TemporaryDirectory() as directory:
            prog_a = pathlib.Path(directory) / 'a.hy'
            prog_a.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            original = {
                'version': 1,
                'blocks': {
                    f'{prog_a}:1': block('a_old'),
                    'unrelated.hy:10': block('unrelated_stay'),
                }
            }
            baseline.write_text(json.dumps(original))
            report = {'summary': {'managed': {'managed_blocks': 0}}}
            result = subprocess.CompletedProcess([], 0, json.dumps(report).encode('utf-8'), b'')
            output = io.StringIO()
            argv = ['managed_gate.py', 'hyb', str(baseline), str(prog_a)]
            with patch.object(gate.sys, 'argv', argv), \
                 patch.object(gate.subprocess, 'run', return_value=result), \
                 contextlib.redirect_stdout(output):
                gate.main()
            out = output.getvalue()
            self.assertIn(f'managed block removed (allowed): {prog_a}:1', out)
            self.assertNotIn('unrelated.hy', out)
            self.assertIn('managed gate: PASS', out)

    def test_rebaseline_subset_preserves_unrelated_baseline_blocks(self):
        with tempfile.TemporaryDirectory() as directory:
            prog_a = pathlib.Path(directory) / 'a.hy'
            prog_a.write_text('fn main() {}', encoding='utf-8')
            baseline = pathlib.Path(directory) / 'baseline.json'
            original = {
                'version': 1,
                'blocks': {
                    f'{prog_a}:1': block('a_old'),
                    'unrelated.hy:10': block('unrelated_stay'),
                }
            }
            baseline.write_text(json.dumps(original))
            report = {'summary': {'managed': {'managed_blocks': 0}}}
            result = subprocess.CompletedProcess([], 0, json.dumps(report).encode('utf-8'), b'')
            output = io.StringIO()
            argv = ['managed_gate.py', '--rebaseline', 'hyb', str(baseline), str(prog_a)]
            with patch.object(gate.sys, 'argv', argv), \
                 patch.object(gate.subprocess, 'run', return_value=result), \
                 contextlib.redirect_stdout(output):
                gate.main()
            saved = json.loads(baseline.read_text())['blocks']
            self.assertNotIn(f'{prog_a}:1', saved)
            self.assertEqual(saved.get('unrelated.hy:10'), block('unrelated_stay'))


class ManagedGateUtf8Tests(unittest.TestCase):
    """Exercise actual checker pipes, including a legacy Windows fallback codec."""

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.source = self.root / 'case.hy'
        self.source.write_text('managed("caf\u00e9", "other") {}', encoding='utf-8')
        self.baseline = self.root / 'baseline.json'
        self.checker = self.root / 'checker.py'
        self.checker.write_text(
            'import json, sys\n'
            'case = sys.argv[-1]\n'
            'if case == "invalid-stdout":\n'
            '    sys.stdout.buffer.write(b"\\xff")\n'
            'elif case == "checker-error":\n'
            '    sys.stderr.buffer.write("checker: caf\\u00e9".encode("utf-8"))\n'
            '    raise SystemExit(2)\n'
            'else:\n'
            '    reason = "plain" if case == "ascii" else "caf\\u00e9"\n'
            '    report = {"summary": {"managed": {"managed_blocks": 1,\n'
            '              "blocks": [{"reason": reason, "category": "other",\n'
            '                          "tickets": [reason]}]}}}\n'
            '    sys.stdout.buffer.write(json.dumps(report, ensure_ascii=False).encode("utf-8"))\n'
            '    if case == "invalid-stderr":\n'
            '        sys.stderr.buffer.write(b"\\xff")\n', encoding='utf-8')
        self.real_run = subprocess.run
        self.case = 'unicode'

    def run_checker(self, command, **kwargs):
        # Route the checker executable only; retain real process I/O and decoding.
        # CP936 reproduces the native Windows default on Linux CI as well.
        if kwargs.get('text') or kwargs.get('universal_newlines'):
            kwargs.setdefault('encoding', 'cp936')
        return self.real_run([sys.executable, str(self.checker), self.case], **kwargs)

    def run_gate(self, rebaseline=False):
        argv = ['managed_gate.py'] + (['--rebaseline'] if rebaseline else [])
        argv += ['hyb', str(self.baseline), str(self.source)]
        output = io.StringIO()
        with patch.object(gate.sys, 'argv', argv), \
                patch.object(gate.subprocess, 'run', self.run_checker), \
                contextlib.redirect_stdout(output):
            try:
                gate.main()
            except SystemExit as error:
                return error.code, output.getvalue()
        return 0, output.getvalue()

    def reviewed_baseline(self, reason='caf\u00e9'):
        reviewed = block(reason)
        reviewed['tickets'] = [reason]
        original = json.dumps({'version': 1, 'blocks': {
            f'{self.source}:1': reviewed}}).encode('utf-8')
        self.baseline.write_bytes(original)
        return original

    def test_unicode_reason_and_tickets_survive_real_pipes(self):
        with patch.object(gate.subprocess, 'run', self.run_checker):
            inventory = gate.audit('hyb', str(self.source))
        self.assertEqual(inventory[f'{self.source}:1']['reason'], 'caf\u00e9')
        self.assertEqual(inventory[f'{self.source}:1']['tickets'], ['caf\u00e9'])

    def test_ascii_report_remains_compatible(self):
        self.case = 'ascii'
        original = self.reviewed_baseline('plain')
        code, output = self.run_gate()
        self.assertEqual(code, 0, output)
        self.assertIn('managed gate: PASS', output)
        self.assertEqual(self.baseline.read_bytes(), original)

    def test_unchanged_unicode_baseline_passes_without_rewrite(self):
        original = self.reviewed_baseline()
        code, output = self.run_gate()
        self.assertEqual(code, 0, output)
        self.assertIn('managed gate: PASS', output)
        self.assertEqual(self.baseline.read_bytes(), original)

    def test_rebaseline_records_exact_unicode_report(self):
        self.reviewed_baseline()
        code, output = self.run_gate(rebaseline=True)
        self.assertEqual(code, 0, output)
        saved = json.loads(self.baseline.read_bytes())['blocks'][f'{self.source}:1']
        self.assertEqual(saved['reason'], 'caf\u00e9')
        self.assertEqual(saved['tickets'], ['caf\u00e9'])

    def test_checker_failure_retains_utf8_diagnostic(self):
        self.case = 'checker-error'
        original = self.reviewed_baseline()
        code, output = self.run_gate(rebaseline=True)
        self.assertEqual(code, 2, output)
        self.assertIn('checker: caf\u00e9', output)
        self.assertEqual(self.baseline.read_bytes(), original)

    def test_invalid_utf8_never_passes_or_rewrites_baseline(self):
        for case in ('invalid-stdout', 'invalid-stderr'):
            for rebaseline in (False, True):
                with self.subTest(case=case, rebaseline=rebaseline):
                    self.case = case
                    original = self.reviewed_baseline()
                    code, output = self.run_gate(rebaseline)
                    self.assertEqual(code, 2, output)
                    self.assertIn('GATE ERROR', output)
                    self.assertIn('UTF-8', output)
                    self.assertNotIn('managed gate: PASS', output)
                    self.assertEqual(self.baseline.read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
