"""Regression tests for the test-registration guard (offline, stdlib only).

Each case builds a throwaway repo tree plus a throwaway workflow, so the
guard is exercised against fixtures rather than the live repository. The
guard covers two invocation styles, and both are pinned here:

  tools/**/test_*.py  reached through `unittest discover -s DIR -p PATTERN`
  tests/**/test_*.py  reached by literal repo-relative path
"""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import sys
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

    def test_quoted_directories_and_long_options_keep_their_patterns(self):
        # from pr-540
        self.assertEqual(guard.registered_patterns(
            'python3 -m unittest discover -p "test_*.py" -s "tools/one"\n'
            'python3 -m unittest discover --start-directory=tools/two '
            '--pattern=test_x.py\n'),
            ["test_*.py", "test_x.py"])

    def test_direct_path_match_respects_word_boundaries(self):
        workflow = "          python3 tests/test_install_runtime.py\n"
        self.assertTrue(
            guard.is_registered_direct("tests/test_install_runtime.py", workflow))
        self.assertFalse(
            guard.is_registered_direct("tests/test_install_runtime_extra.py", workflow))

    # --- explicit-invocation parsing (from pr-600, fixes #596) ---

    def test_commented_discovery_is_not_a_registration(self):
        self.assertEqual(guard.registered_patterns(
            "  # python3 -m unittest discover -s tools -p test_disabled.py\n"), [])

    def test_echoed_discovery_is_not_a_registration(self):
        self.assertEqual(guard.registered_patterns(
            "  echo 'python3 -m unittest discover -s tools -p test_disabled.py'\n"), [])

    def test_direct_script_mentions_do_not_register_execution(self):
        for workflow in (
            "# python3 tests/test_disabled.py\n",
            "echo 'python3 tests/test_disabled.py'\n",
            "name: python3 tests/test_disabled.py\n",
            "python3 other.py tests/test_disabled.py\n",
            "python3 tests/test_live.py # tests/test_disabled.py\n",
        ):
            with self.subTest(workflow=workflow):
                self.assertFalse(guard.is_registered_direct("tests/test_disabled.py", workflow))

    def test_inline_run_and_quoted_patterns_are_registered(self):
        self.assertEqual(guard.registered_patterns(
            '  - run: python3 -m unittest discover -s tools -p "test_*.py" # run tests\n'),
            ["test_*.py"])
        self.assertTrue(guard.is_registered_direct(
            "tests/test_live.py", "  run: python3 tests/test_live.py\n"))

    def test_later_shell_command_does_not_supply_discovery_patterns(self):
        for separator in (" && ", "&&", ";", " || ", " | "):
            with self.subTest(separator=separator):
                self.assertEqual(guard.registered_patterns(
                    "python3 -m unittest discover -s tools -p test_live.py"
                    + separator + "echo -p test_disabled.py\n"), ["test_live.py"])

    def test_interpreter_flags_preserve_real_invocations(self):
        self.assertEqual(guard.registered_patterns(
            "python3 -B -W ignore -X dev -m unittest discover -s tools -p test_live.py\n"),
            ["test_live.py"])
        self.assertTrue(guard.is_registered_direct(
            "tests/test_live.py", "python3 -u tests/test_live.py\n"))
        self.assertFalse(guard.is_registered_direct(
            "tests/test_live.py", "python3 -c 'print(1)' tests/test_live.py\n"))

    def test_repeated_pattern_uses_the_last_value_like_unittest(self):
        self.assertEqual(guard.registered_patterns(
            "python3 -m unittest discover -s tools -p test_disabled.py -p test_live.py\n"),
            ["test_live.py"])


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

    # --- discovery start-directory scoping (from pr-602, fixes #574; pr-705) ---

    def test_matching_basename_outside_start_directory_is_unregistered(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools/fuzz -p 'test_*.py'\n",
            ["tools/test_orphan.py", "tools/fuzz/test_control.py",
             "tools/runner/test_runner.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/runner/test_runner.py", "tools/test_orphan.py"])

    def test_discovery_does_not_descend_into_non_package_directory(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools -p 'test_*.py'\n",
            ["tools/test_control.py", "tools/fuzz/test_orphan.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/fuzz/test_orphan.py"])
        # An explicit start directory needs no __init__.py of its own.
        text += "python3 -m unittest discover -s tools/fuzz -p 'test_*.py'\n"
        self.assertEqual(guard.unregistered(root, text), [])

    def test_discovery_descends_through_packages_only(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools -p 'test_*.py'\n",
            ["tools/pkg/__init__.py", "tools/pkg/test_one.py",
             "tools/pkg/deep/test_two.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/pkg/deep/test_two.py"])
        (root / "tools/pkg/deep/__init__.py").write_text("", encoding="utf-8")
        self.assertEqual(guard.unregistered(root, text), [])

    def test_every_intermediate_directory_must_be_a_package(self):
        # from pr-540
        root, text = self.build(
            "python3 -m unittest discover -s tools -p 'test_*.py'\n",
            ["tools/plain/package/test_x.py"])
        (root / "tools/plain/package/__init__.py").touch()
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/plain/package/test_x.py"])

    def test_patterns_stay_paired_with_their_start_directories(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools/fuzz -p test_one.py\n"
            "python3 -m unittest discover -s tools/runner -p test_two.py\n",
            ["tools/fuzz/test_one.py", "tools/fuzz/test_two.py",
             "tools/runner/test_two.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/fuzz/test_two.py"])

    def test_quoted_paths_and_reordered_flags_keep_their_scope(self):
        root, text = self.build(
            'python3 -m unittest discover -p "test_*.py" -s "./tools/with space"\n',
            ["tools/with space/test_one.py", "tools/test_orphan.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/test_orphan.py"])

    def test_file_cannot_be_a_start_directory(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools/test_one.py -p test_one.py\n",
            ["tools/test_one.py"])
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/test_one.py"])

    def test_actual_discovery_skips_failing_test_but_guard_must_reject_it(self):
        root, text = self.build(
            "python3 -m unittest discover -s tools/fuzz -p 'test_*.py'\n",
            ["tools/test_orphan.py", "tools/fuzz/test_control.py"])
        (root / "tools/test_orphan.py").write_text(
            "import unittest\nclass Orphan(unittest.TestCase):\n"
            "    def test_failure(self): self.fail('orphan really ran')\n",
            encoding="utf-8")
        (root / "tools/fuzz/test_control.py").write_text(
            "import unittest\nclass Control(unittest.TestCase):\n"
            "    def test_pass(self): pass\n", encoding="utf-8")
        command = [sys.executable, "-m", "unittest", "discover", "-s"]
        skipped = subprocess.run(command + ["tools/fuzz", "-p", "test_*.py"],
                                 cwd=root, capture_output=True, text=True, timeout=15)
        self.assertEqual(skipped.returncode, 0, skipped.stderr)
        self.assertIn("Ran 1 test", skipped.stderr)
        exercised = subprocess.run(command + ["tools", "-p", "test_orphan.py"],
                                   cwd=root, capture_output=True, text=True, timeout=15)
        self.assertNotEqual(exercised.returncode, 0)
        self.assertIn("orphan really ran", exercised.stderr)
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/test_orphan.py"])

    # --- explicit-invocation end-to-end (from pr-600, fixes #596) ---

    def test_successful_shell_with_disabled_tests_must_fail_registration(self):
        # The shell succeeds without running either deliberately failing test.
        # The guard must detect the missing execution, not approve the mentions.
        commands = (
            "# python3 -m unittest discover -s tools -p test_disabled.py\n"
            "echo 'python3 tests/test_disabled.py'\n"
        )
        root, text = self.build(commands, ["tools/test_disabled.py", "tests/test_disabled.py"])
        for rel in ("tools/test_disabled.py", "tests/test_disabled.py"):
            (root / rel).write_text(
                "raise AssertionError('this test must be executed')\n", encoding="utf-8")
        if os.name != "nt":
            shell = subprocess.run(["/bin/sh", "-c", commands], cwd=root,
                                   capture_output=True, text=True, timeout=10)
            self.assertEqual(shell.returncode, 0, shell.stderr)
        self.assertEqual([rel for rel, _ in guard.unregistered(root, text)],
                         ["tools/test_disabled.py", "tests/test_disabled.py"])


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

    def test_matching_orphan_outside_discovery_directory_exits_one(self):
        self.assertEqual(
            self.run_main("python3 -m unittest discover -s tools/fuzz -p 'test_*.py'\n",
                          ["tools/fuzz/test_control.py", "tools/test_orphan.py"]), 1)


    def test_start_directory_escaping_repo_root_is_rejected(self):
        self.assertEqual(self.run_main("python3 -m unittest discover -s .. -p 'test_*.py'\n", ["tools/test_x.py"]), 1)


if str(Path(__file__).parent) not in sys.path:
    sys.path.insert(0, str(Path(__file__).parent))
import author_stage


class AuthorStageTests(unittest.TestCase):
    """Regression tests for tools/author_stage.py (#1123: diff_paths, scope_ok, gh_token)."""

    def test_diff_paths_extracts_both_plus_and_minus_headers_and_strips_timestamps(self):
        diff = (
            "diff --git a/compiler/crates/hy-check/src/lib.rs b/compiler/crates/hy-check/src/lib.rs\n"
            "--- a/compiler/crates/hy-check/src/lib.rs\t2026-10-22 12:00:00 +0000\n"
            "+++ b/compiler/crates/hy-check/src/lib.rs\t2026-10-22 12:00:01 +0000\n"
            "@@ -1,2 +1,2 @@\n"
            "--- comment deleted inside hunk\n"
            "---not-a-header\n"
            "+++ counter added inside hunk\n"
            "+++not-a-header\n"
            "--- /dev/null\t1970-01-01 00:00:00 +0000\n"
            "+++ b/tests/checker-corpus/new_reject.hy\n"
        )
        self.assertEqual(
            author_stage.diff_paths(diff),
            [
                "compiler/crates/hy-check/src/lib.rs",
                "tests/checker-corpus/new_reject.hy",
            ],
        )

    def test_diff_paths_ignores_triple_dash_and_plus_inside_hunks(self):
        in_scope_hunk_with_triple_markers = (
            "diff --git a/compiler/crates/hy-check/src/lib.rs b/compiler/crates/hy-check/src/lib.rs\n"
            "--- a/compiler/crates/hy-check/src/lib.rs\n"
            "+++ b/compiler/crates/hy-check/src/lib.rs\n"
            "@@ -10,2 +10,2 @@\n"
            "--- comment that starts with three dashes\n"
            "-x\n"
            "+++ comment that starts with three pluses\n"
            "+y\n"
        )
        self.assertEqual(
            author_stage.diff_paths(in_scope_hunk_with_triple_markers),
            ["compiler/crates/hy-check/src/lib.rs"],
        )
        self.assertTrue(author_stage.scope_ok(in_scope_hunk_with_triple_markers))

    def test_diff_paths_and_scope_ok_extract_git_rename_and_copy_headers(self):
        rename_diff = (
            "diff --git a/compiler/crates/hy-check/src/old.rs b/compiler/crates/hy-check/src/new.rs\n"
            "similarity index 98%\n"
            "rename from compiler/crates/hy-check/src/old.rs\n"
            "rename to compiler/crates/hy-check/src/new.rs\n"
        )
        self.assertEqual(
            author_stage.diff_paths(rename_diff),
            [
                "compiler/crates/hy-check/src/old.rs",
                "compiler/crates/hy-check/src/new.rs",
            ],
        )
        self.assertTrue(author_stage.scope_ok(rename_diff))

        copy_diff = (
            "diff --git a/compiler/crates/hy-check/src/orig.rs b/compiler/crates/hy-check/src/clone.rs\n"
            "similarity index 100%\n"
            "copy from compiler/crates/hy-check/src/orig.rs\n"
            "copy to compiler/crates/hy-check/src/clone.rs\n"
        )
        self.assertEqual(
            author_stage.diff_paths(copy_diff),
            [
                "compiler/crates/hy-check/src/orig.rs",
                "compiler/crates/hy-check/src/clone.rs",
            ],
        )
        self.assertTrue(author_stage.scope_ok(copy_diff))

    def test_scope_ok_rejects_deletions_or_mutations_outside_scope(self):
        mixed_delete = (
            "--- a/.github/workflows/ci.yml\n"
            "+++ /dev/null\n"
            "--- a/compiler/crates/hy-check/src/lib.rs\n"
            "+++ b/compiler/crates/hy-check/src/lib.rs\n"
        )
        only_delete = (
            "--- a/.github/workflows/ci.yml\n"
            "+++ /dev/null\t1970-01-01 00:00:00 +0000\n"
        )
        empty_file_delete_git_header_only = (
            "diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml\n"
            "deleted file mode 100644\n"
            "index e69de29..0000000\n"
        )
        mode_change_only = (
            "diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml\n"
            "old mode 100644\n"
            "new mode 100755\n"
        )
        new_empty_file_only = (
            "diff --git a/tools/pwned.py b/tools/pwned.py\n"
            "new file mode 100644\n"
            "index 0000000..e69de29\n"
        )
        binary_patch_only = (
            "diff --git a/tools/test_pwn.py b/tools/test_pwn.py\n"
            "new file mode 100644\n"
            "GIT binary patch\n"
            "literal 0\n"
        )
        rename_to_out_of_scope = (
            "diff --git a/compiler/crates/hy-check/src/lib.rs b/tools/autoland.py\n"
            "similarity index 100%\n"
            "rename from compiler/crates/hy-check/src/lib.rs\n"
            "rename to tools/autoland.py\n"
        )
        copy_to_out_of_scope = (
            "copy from compiler/crates/hy-check/src/lib.rs\n"
            "copy to tools/autoland.py\n"
        )
        self.assertFalse(author_stage.scope_ok(mixed_delete))
        self.assertFalse(author_stage.scope_ok(only_delete))
        self.assertFalse(author_stage.scope_ok(empty_file_delete_git_header_only))
        self.assertFalse(author_stage.scope_ok(mode_change_only))
        self.assertFalse(author_stage.scope_ok(new_empty_file_only))
        self.assertFalse(author_stage.scope_ok(binary_patch_only))
        self.assertFalse(author_stage.scope_ok(rename_to_out_of_scope))
        self.assertFalse(author_stage.scope_ok(copy_to_out_of_scope))

    def test_scope_ok_rejects_dotdot_traversal_backslash_leading_slash_p1_and_symlink(self):
        for bad_diff in (
            "+++ b/compiler/../tools/autoland.py\n",
            "--- a/tests/../../.github/workflows/ci.yml\n+++ b/compiler/crates/hy-check/src/lib.rs\n",
            "+++ b/compiler\\..\\tools\\autoland.py\n",
            "+++ b/compiler/crates\\lib.rs\n",
            "+++ /compiler/crates/hy-check/src/lib.rs\n",
            "+++ b//compiler/crates/hy-check/src/lib.rs\n",
            "--- compiler/tools/autoland.py\n+++ compiler/tools/autoland.py\n",
            '+++ "b/.github/workflows/ci.yml"\n',
            '+++ "b/compiler/\\056\\056/tools/autoland.py"\n',
            '+++ b/compiler/crates/"hy-check"/src/lib.rs\n',
            'diff --git "a/compiler/x.rs" "b/tools/autoland.py"\n',
            "diff --git a/compiler/link.rs b/compiler/link.rs\nnew file mode 120000\n--- /dev/null\n+++ b/compiler/link.rs\n",
            "+++ b/compiler/crates/hy-check/src/lib\x00.rs\n",
            "+++ b/compiler/crates/hy-check/src/lib\x1f.rs\n",
            "+++ b/compiler/crates/hy-check/src/lib\x7f.rs\n",
        ):
            with self.subTest(bad_diff=bad_diff):
                self.assertFalse(author_stage.scope_ok(bad_diff))

    def test_scope_ok_accepts_valid_in_scope_modifications_additions_and_deletions(self):
        for good_diff in (
            "--- a/compiler/crates/hy-check/src/lib.rs\n+++ b/compiler/crates/hy-check/src/lib.rs\n",
            "--- /dev/null\n+++ b/tests/checker-corpus/new_reject.hy\n",
            "--- a/automation/land-queue/author-1/repro.hy\n+++ /dev/null\n",
            "--- a/tests/checker-corpus/range..expr.hy\n+++ b/tests/checker-corpus/range..expr.hy\n",
            '--- "a/compiler/crates/hy-check/src/lib.rs"\n+++ "b/compiler/crates/hy-check/src/lib.rs"\n',
            "diff --git a/compiler/crates/hy-check/src/a.rs b/compiler/crates/hy-check/src/b.rs\n"
            "rename from compiler/crates/hy-check/src/a.rs\n"
            "rename to compiler/crates/hy-check/src/b.rs\n",
        ):
            with self.subTest(good_diff=good_diff):
                self.assertTrue(author_stage.scope_ok(good_diff))

    def test_gh_token_returns_env_token_when_set(self):
        saved = {k: os.environ.pop(k, None) for k in ("GITHUB_TOKEN", "GH_TOKEN")}
        try:
            os.environ["GH_TOKEN"] = "tok_env_fallback"
            self.assertEqual(author_stage.gh_token(), "tok_env_fallback")
            os.environ["GITHUB_TOKEN"] = "   \n\t  "
            self.assertEqual(author_stage.gh_token(), "tok_env_fallback")
            os.environ["GITHUB_TOKEN"] = "  tok_primary_env \n"
            self.assertEqual(author_stage.gh_token(), "tok_primary_env")
        finally:
            for k in ("GITHUB_TOKEN", "GH_TOKEN"):
                os.environ.pop(k, None)
            for k, v in saved.items():
                if v is not None:
                    os.environ[k] = v

    def test_gh_token_unpacks_completed_process_stdout_and_checks_returncode(self):
        saved_env = {k: os.environ.pop(k, None) for k in ("GITHUB_TOKEN", "GH_TOKEN")}
        orig_sh = author_stage.sh
        orig_root = author_stage.ROOT
        try:
            author_stage.sh = lambda *a, **kw: subprocess.CompletedProcess(
                args=["git", "remote", "get-url", "origin"],
                returncode=0,
                stdout="https://tok_remote_embedded_123@github.com/limitlessprojects/Go-Hybrid.git\n",
                stderr="",
            )
            self.assertEqual(author_stage.gh_token(), "tok_remote_embedded_123")

            author_stage.sh = lambda *a, **kw: subprocess.CompletedProcess(
                args=["git", "remote", "get-url", "origin"],
                returncode=0,
                stdout="https://x-access-token:tok_oauth_456@github.com/limitlessprojects/Go-Hybrid.git\n",
                stderr="",
            )
            self.assertEqual(author_stage.gh_token(), "tok_oauth_456")

            with tempfile.TemporaryDirectory(prefix="hybrid-author-tok-") as tmp:
                author_stage.ROOT = Path(tmp)
                author_stage.sh = lambda *a, **kw: subprocess.CompletedProcess(
                    args=["git", "remote", "get-url", "origin"],
                    returncode=128,
                    stdout="https://tok_should_be_ignored_on_nonzero_rc@github.com/limitlessprojects/Go-Hybrid.git\n",
                    stderr="fatal: not a git repository",
                )
                tok = author_stage.gh_token()
                self.assertNotEqual(tok, "tok_should_be_ignored_on_nonzero_rc")
                self.assertTrue(tok is None or isinstance(tok, str))

                author_stage.sh = lambda *a, **kw: subprocess.CompletedProcess(
                    args=["git", "remote", "get-url", "origin"],
                    returncode=0,
                    stdout=None,
                    stderr="",
                )
                tok2 = author_stage.gh_token()
                self.assertTrue(tok2 is None or isinstance(tok2, str))
        finally:
            author_stage.sh = orig_sh
            author_stage.ROOT = orig_root
            for k, v in saved_env.items():
                if v is not None:
                    os.environ[k] = v

    def test_selftest_and_cli_flags_succeed(self):
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(author_stage.selftest(), 0)
        script = str(Path(__file__).with_name("author_stage.py"))
        for arg in ("selftest", "--selftest"):
            with self.subTest(arg=arg):
                proc = subprocess.run(
                    [sys.executable, script, arg],
                    capture_output=True,
                    text=True,
                    timeout=30,
                )
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn("SELFTEST PASS", proc.stdout)


if __name__ == "__main__":
    unittest.main()

