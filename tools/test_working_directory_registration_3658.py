"""Regression tests for Issue #3658: working-directory test registration boundary."""

import unittest
from pathlib import Path
import tempfile
import sys
import os

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "tools"))

import check_test_registration

class WorkingDirectoryRegistrationTests(unittest.TestCase):
    def test_step_working_directory_does_not_register_root_tests(self):
        wf = '''name: repro
on: push
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - working-directory: nested
        run: python3 -m unittest discover -s tools -p test_target.py
'''
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            p = root / "tools" / "test_target.py"
            p.parent.mkdir(parents=True)
            p.write_text("import unittest\n", encoding="utf-8")
            missing = check_test_registration.unregistered(root, wf)
            self.assertEqual([rel for rel, _ in missing], ["tools/test_target.py"])

    def test_job_defaults_working_directory_does_not_register_root_tests(self):
        wf = '''name: repro
on: push
jobs:
  check:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: nested
    steps:
      - run: python3 -m unittest discover -s tools -p test_target.py
'''
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            p = root / "tools" / "test_target.py"
            p.parent.mkdir(parents=True)
            p.write_text("import unittest\n", encoding="utf-8")
            missing = check_test_registration.unregistered(root, wf)
            self.assertEqual([rel for rel, _ in missing], ["tools/test_target.py"])

    def test_workflow_defaults_working_directory_does_not_register_root_tests(self):
        wf = '''name: repro
defaults:
  run:
    working-directory: nested
on: push
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - run: python3 -m unittest discover -s tools -p test_target.py
'''
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            p = root / "tools" / "test_target.py"
            p.parent.mkdir(parents=True)
            p.write_text("import unittest\n", encoding="utf-8")
            missing = check_test_registration.unregistered(root, wf)
            self.assertEqual([rel for rel, _ in missing], ["tools/test_target.py"])

    def test_root_working_directory_registers_root_tests(self):
        for wd in (".", "./", "${{ github.workspace }}"):
            with self.subTest(wd=wd):
                wf = f'''name: repro
on: push
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - working-directory: {wd}
        run: python3 -m unittest discover -s tools -p test_target.py
'''
                with tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    p = root / "tools" / "test_target.py"
                    p.parent.mkdir(parents=True)
                    p.write_text("import unittest\n", encoding="utf-8")
                    missing = check_test_registration.unregistered(root, wf)
                    self.assertEqual(missing, [])

if __name__ == "__main__":
    unittest.main()
