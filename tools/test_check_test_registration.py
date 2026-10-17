"""Regression tests for the test-registration guard (offline, stdlib only).

Each case builds a throwaway repo tree plus a throwaway workflow, so the
guard is exercised against fixtures rather than the live repository. The
guard covers two invocation styles, and both are pinned here:

  tools/**/test_*.py  reached through `unittest discover -p PATTERN`
  tests/**/test_*.py  reached by literal repo-relative path
"""
import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "check_test_registration", Path(__file__).with_name("check_test_registration.py"))
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


class PatternExtractionTests(unittest.TestCase):
    def test_quoted_and_bare_patterns_are_both_read(self):
        workflow = (
            "          python3 -m unittest discover -s tools/fuzz -p 'test_hyfuzz.py'\n"
            "          python3 -m unittest discover -s tools -p test_managed_gate.py -v\n"
        )
        self.assertEqual(guard.registered_patterns(workflow),
                         ["test_hyfuzz.py", "test_managed_gate.py"])

    def test_non_discovery_python_lines_are_ignored(self):
        workflow = (
            "          python3 -m py_compile tools/job.py tools/corpus_harness.py\n"
            "          python3 tests/test_install_checksums.py\n"
            "          python3 tools/runner/runner.py job.json\n"
        )
        self.assertEqual(guard.registered_patterns(workflow), [])

    def test_direct_path_match_respects_word_boundaries(self):
        workflow = "          python3 tests/test_install_runtime.py\n"
        self.assertTrue(
            guard.is_registered_direct("tests/test_install_runtime.py", workflow))
        self.assertFalse(
            guard.is_registered_direct("tests/test_install_runtime_extra.py", workflow))


class RegistrationTests(unittest.TestCase):
    def build(self, workflow, suites):
        """Write a temp root with `suites` and return (root, workflow_text)."""
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-registration-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for rel in suites:
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("# suite\n", encoding="utf-8")
        return root, workflow

    def test_glob_pattern_registers_a_whole_family(self):
        workflow = "          python3 -m unittest discover -s tools -p 'test_job*.py'\n"
        root, text = self.build(
            workflow,
            ["tools/test_job_cli.py", "tools/test_job_payload.py", "tools/test_job_artifacts.py"])
        self.assertEqual(guard.find_test_files(root, "tools"), [
            "tools/test_job_artifacts.py", "tools/test_job_cli.py", "tools/test_job_payload.py"])
        self.assertEqual(guard.unregistered(root, text), [])

    def test_unregistered_suite_is_detected(self):
        workflow = "          python3 -m unittest discover -s tools -p test_job_cli.py -v\n"
        root, text = self.build(workflow, ["tools/test_job_cli.py", "tools/test_job_payload.py"])
        missing = guard.unregistered(root, text)
        self.assertEqual([rel for rel, _ in missing], ["tools/test_job_payload.py"])

    def test_nested_suite_is_found_and_checked(self):
        workflow = "          python3 -m unittest discover -s tools -p test_none.py\n"
        root, text = self.build(
            workflow, ["tools/fuzz/test_hyfuzz.py", "tools/runner/test_runner.py"])
        self.assertEqual(guard.find_test_files(root, "tools"),
                         ["tools/fuzz/test_hyfuzz.py", "tools/runner/test_runner.py"])
        self.assertEqual(sorted(rel for rel, _ in guard.unregistered(root, text)),
                         ["tools/fuzz/test_hyfuzz.py", "tools/runner/test_runner.py"])

    def test_direct_script_is_registered_only_when_its_path_is_named(self):
        root, text = self.build(
            "          python3 tests/test_install_runtime.py\n",
            ["tests/test_install_runtime.py"])
        self.assertEqual(guard.unregistered(root, text), [])

    def test_uninvoked_direct_script_is_detected(self):
        root, text = self.build(
            "          python3 -m unittest discover -s tools -p test_x.py\n",
            ["tests/test_install_checksums.py"])
        missing = guard.unregistered(root, text)
        self.assertEqual([rel for rel, _ in missing], ["tests/test_install_checksums.py"])

    def test_both_halves_are_checked_together(self):
        workflow = "          python3 -m unittest discover -s tools -p test_x.py\n"
        root, text = self.build(
            workflow, ["tools/test_x.py", "tests/test_install_upgrade.py"])
        missing = guard.unregistered(root, text)
        self.assertEqual([rel for rel, _ in missing], ["tests/test_install_upgrade.py"])


class GuardMisuseTests(unittest.TestCase):
    def run_main(self, workflow, suites=()):
        """Run the real main() against a throwaway workflow/root pair."""
        with tempfile.TemporaryDirectory(prefix="hybrid-guard-") as temporary:
            root = Path(temporary)
            path = root / "ci.yml"
            path.write_text(workflow, encoding="utf-8")
            for rel in suites:
                target = root / rel
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text("# suite\n", encoding="utf-8")
            if not suites:
                (root / "tools").mkdir()
            original = guard.sys.argv
            guard.sys.argv = ["check_test_registration.py",
                              "--workflow", str(path), "--root", str(root)]
            try:
                with contextlib.redirect_stdout(io.StringIO()), \
                        contextlib.redirect_stderr(io.StringIO()):
                    return guard.main()
            finally:
                guard.sys.argv = original

    def test_workflow_without_any_reference_fails_closed(self):
        self.assertEqual(
            self.run_main("python3 -m py_compile tools/job.py\n", ["tools/test_x.py"]), 2)

    def test_root_without_suites_fails_closed(self):
        self.assertEqual(
            self.run_main("python3 -m unittest discover -s tools -p test_x.py\n"), 2)

    def test_fully_registered_tree_passes(self):
        self.assertEqual(
            self.run_main("python3 -m unittest discover -s tools -p test_x.py\n",
                          ["tools/test_x.py"]), 0)

    def test_orphaned_suite_exits_one(self):
        self.assertEqual(
            self.run_main("python3 -m unittest discover -s tools -p test_x.py\n",
                          ["tools/test_x.py", "tools/test_y.py"]), 1)


if __name__ == "__main__":
    unittest.main()
