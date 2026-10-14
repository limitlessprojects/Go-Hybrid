import subprocess
import json
import contextlib
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('harness', Path(__file__).with_name('corpus_harness.py'))
harness = importlib.util.module_from_spec(spec)
spec.loader.exec_module(harness)


class CorpusTests(unittest.TestCase):
    def run_case(self, source):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'case.hy').write_text(source, encoding='utf-8')
            binary = root / 'hyb'
            binary.write_text('unused', encoding='utf-8')
            with patch.object(sys, 'argv', ['harness', '--bin', str(binary), '--corpus', folder]), patch.object(harness, 'run_checker', return_value=({'verdict': 'accept'}, [])) as check, contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                result = harness.main()
                return result, check.call_count

    def test_all_unregistered_is_not_success(self):
        self.assertEqual(self.run_case('fn main() {}\n'), (2, 0))

    def test_registered_case_still_runs(self):
        self.assertEqual(self.run_case('// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n'), (0, 1))



    def test_unexpected_checker_exit_causes_harness_error(self):
        payload = {"summary": {"verdict": "accept", "errors": 0}, "diagnostics": []}
        child = subprocess.CompletedProcess(
            args=["fake"], returncode=2, stdout=json.dumps(payload), stderr="usage error"
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'case.hy').write_text('// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n', encoding='utf-8')
            binary = root / 'hyb'
            binary.write_text('unused', encoding='utf-8')
            with patch.object(sys, 'argv', ['harness', '--bin', str(binary), '--corpus', folder]), \
                 patch.object(harness.subprocess, 'run', return_value=child), \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                result = harness.main()
                self.assertEqual(result, 1)

    def test_normal_rejection_exit_code_1_is_handled(self):
        payload = {"summary": {"verdict": "reject", "errors": 1}, "diagnostics": [{"severity": "error", "tier0_rule": "A1b"}]}
        child = subprocess.CompletedProcess(
            args=["fake"], returncode=1, stdout=json.dumps(payload), stderr=""
        )
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'case.hy').write_text('// EXPECTED-VERDICT: REJECT A1b\nfn main() {}\n', encoding='utf-8')
            binary = root / 'hyb'
            binary.write_text('unused', encoding='utf-8')
            with patch.object(sys, 'argv', ['harness', '--bin', str(binary), '--corpus', folder]), \
                 patch.object(harness.subprocess, 'run', return_value=child), \
                 contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                result = harness.main()
                self.assertEqual(result, 0)

if __name__ == '__main__':
    unittest.main()
