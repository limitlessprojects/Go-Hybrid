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
import shlex
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

    def test_unfiltered_direct_options_and_paths_still_register(self):
        for path, command in (
            ("tests/test_live.py", "python3 tests/test_live.py -vqbcf --locals"),
            ("tests/test_live.py", "python3 -B -W ignore -X dev -- ./tests/test_live.py -v"),
            ("tests/-k suite/test_live.py", "python3 'tests/-k suite/test_live.py' -v"),
            ("tests/test_live.py", "run: 'python3 tests/test_live.py -v'"),
            ("tests/test_live.py", "python3 tests/test_live.py # -k test_pass"),
            ("tests/test_live.py", "python3 tests/test_live.py; echo -k test_pass"),
            ("tests/test_live.py", "python3 tests/test_live.py && echo -k test_pass"),
        ):
            with self.subTest(command=command):
                self.assertTrue(guard.is_registered_direct(path, command))

    def test_direct_filters_remain_visible_around_redirections(self):
        path = "tests/test_live.py"
        for arguments in (
            path + " > output -k test_pass",
            path + " -k test_pass > output",
            path + " -k > output test_pass",
            path + " >> output -vbqktest_pass",
            path + " 2> errors -ktest_pass",
            path + " 2>&1 -k test_pass",
            path + " < input -k test_pass",
            path + " <> input -k test_pass",
            path + " >| output -k test_pass",
            path + " &> output -k test_pass",
            path + " &>> output -k test_pass",
            path + " 3<&0 -k test_pass",
            path + " > first 2> second -k test_pass",
            "> output " + path + " -k test_pass",
            "2> errors ./" + path + " -k test_pass",
        ):
            with self.subTest(arguments=arguments):
                filtered = "python3 -B " + arguments
                self.assertFalse(guard.is_registered_direct(path, filtered))
                self.assertTrue(guard.is_registered_direct(
                    path, filtered + "\npython3 " + path + " > complete\n"))

    def test_direct_redirection_targets_and_later_commands_are_not_filters(self):
        path = "tests/test_live.py"
        for arguments in (
            path + " > -k",
            path + " >> -ktest_pass",
            path + " 2> '-vk test_pass'",
            path + " < -k",
            path + " 2>&1",
            path + " &> -k",
            path + " &>> -k",
            path + " >| -k",
            "> output " + path + " -v",
            "2> errors ./" + path + " -v",
            path + " > output # -k test_pass",
            path + " > output; echo -k test_pass",
            path + " > output && echo -k test_pass",
            path + " > output || echo -k test_pass",
            path + " > output | echo -k test_pass",
            path + " > output & echo -k test_pass",
        ):
            with self.subTest(arguments=arguments):
                self.assertTrue(guard.is_registered_direct(path, "python3 -B " + arguments))
        for separator in (";", "&&", "||", "|", "&"):
            with self.subTest(separator=separator):
                self.assertFalse(guard.is_registered_direct(path,
                    "python3 " + path + " > output -k test_pass " + separator +
                    " python3 " + path))

    def test_unsupported_or_incomplete_direct_redirections_do_not_register(self):
        for suffix in (">", "2>", "> ; echo done", "<< EOF", "<<< text", ">(echo data)"):
            with self.subTest(suffix=suffix):
                self.assertFalse(guard.is_registered_direct(
                    "tests/test_live.py", "python3 tests/test_live.py " + suffix))

    def test_direct_descriptors_require_unquoted_adjacent_numbers(self):
        path = "tests/test_live.py"
        for number in ("2 ", "'2'", '"2"', r"\2", '2""'):
            with self.subTest(number=number):
                self.assertFalse(guard.is_registered_direct(path,
                    "python3 -B " + number + "> output " + path))
        self.assertTrue(guard.is_registered_direct(path,
            "python3 -B 2> output " + path))
        self.assertFalse(guard.is_registered_direct(path,
            "python3 -B 2> output " + path + " -k test_pass"))

    def test_combined_redirects_do_not_take_descriptor_prefixes(self):
        path = "tests/test_live.py"
        for operator in ("&>", "&>>"):
            with self.subTest(operator=operator):
                self.assertFalse(guard.is_registered_direct(path,
                    "python3 -B 2" + operator + " output " + path))
                self.assertTrue(guard.is_registered_direct(path,
                    "python3 -B " + operator + " output " + path))

    def test_direct_quoted_punctuation_targets_are_words(self):
        path = "tests/test_live.py"
        for target in ("'&'", "';'", "'>'", "'<'", "'>>'", "'&&'", "'|'", "'('",
                       r"\&", r"\>", "'&'\"\"", "'file name'"):
            with self.subTest(target=target):
                complete = "python3 " + path + " > " + target
                self.assertTrue(guard.is_registered_direct(path, complete))
                self.assertFalse(guard.is_registered_direct(path, complete + " -k test_pass"))
                self.assertTrue(guard.is_registered_direct(path, complete + "; echo -k test_pass"))
        self.assertTrue(guard.is_registered_direct(path,
            'run: "python3 ' + path + ' > \'&\'"'))

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

    def test_direct_name_filter_cannot_hide_a_failing_regression(self):
        path = "tests/test_direct_filter.py"
        complete = "python3 " + path + "\n"
        root, _ = self.build(complete, [path])
        (root / path).write_text(
            "import unittest\n"
            "class DirectTests(unittest.TestCase):\n"
            "    def test_failure(self): self.fail('direct regression ran')\n"
            "    def test_pass(self): pass\n"
            "if __name__ == '__main__': unittest.main()\n",
            encoding="utf-8")
        command = [sys.executable, path]
        control = subprocess.run(command, cwd=root, capture_output=True,
                                 text=True, timeout=15)
        self.assertEqual(control.returncode, 1, control.stderr)
        self.assertIn("Ran 2 tests", control.stderr)
        self.assertIn("direct regression ran", control.stderr)
        for options, count in (
            (["-k", "test_pass"], 1),
            (["-ktest_pass"], 1),
            (["-vk", "test_pass"], 1),
            (["-qktest_pass"], 1),
            (["-bfck", "test_pass"], 1),
            (["-vbqfk", "test_pass"], 1),
            (["-k", "never_matches", "-k", "test_pass"], 1),
            (["-k", "test_pass", "-k", "never_matches"], 1),
            (["-k", "never_matches"], 0),
        ):
            with self.subTest(options=options):
                selected = subprocess.run(command + options, cwd=root,
                                          capture_output=True, text=True, timeout=15)
                # Python 3.12+ reports an empty selection with exit 5; a
                # passing subset still exits zero while omitting the failure.
                expected = 5 if count == 0 and sys.version_info >= (3, 12) else 0
                self.assertEqual(selected.returncode, expected, selected.stderr)
                self.assertIn(f"Ran {count} test", selected.stderr)
                self.assertNotIn("direct regression ran", selected.stderr)
                filtered = complete.rstrip() + " " + " ".join(options) + "\n"
                self.assertEqual([rel for rel, _ in guard.unregistered(root, filtered)],
                                 [path])
                self.assertEqual(guard.unregistered(root, filtered + complete), [])

    def test_redirected_direct_filter_cannot_hide_a_failing_regression(self):
        path = "tests/test_redirected.py"
        root, _ = self.build("", [path])
        (root / path).write_text(
            "import unittest\n"
            "class RedirectedTests(unittest.TestCase):\n"
            "    def test_failure(self): self.fail('redirected regression ran')\n"
            "    def test_pass(self): pass\n"
            "if __name__ == '__main__': unittest.main()\n", encoding="utf-8")
        for suffix, options, count in (
            ("> -k", [], 2),
            ("> output -k test_pass", ["-k", "test_pass"], 1),
            ("-k test_pass > output", ["-k", "test_pass"], 1),
            ("-k > output test_pass", ["-k", "test_pass"], 1),
            ("> output -vbqktest_pass", ["-vbqktest_pass"], 1),
        ):
            with self.subTest(suffix=suffix):
                # Fixed Python argv plus a stdio handle reproduces redirection
                # without executing an arbitrary shell command in the test.
                with (root / "output").open("w", encoding="utf-8") as output:
                    actual = subprocess.run([sys.executable, path] + options,
                                            cwd=root, stdout=output, stderr=subprocess.PIPE,
                                            text=True, timeout=15)
                self.assertEqual(actual.returncode, 1 if count == 2 else 0, actual.stderr)
                self.assertIn(f"Ran {count} test", actual.stderr)
                if count == 2:
                    self.assertIn("redirected regression ran", actual.stderr)
                else:
                    self.assertNotIn("redirected regression ran", actual.stderr)
                workflow = "python3 " + path + " " + suffix + "\n"
                self.assertEqual([rel for rel, _ in guard.unregistered(root, workflow)],
                                 [] if count == 2 else [path])

    @unittest.skipIf(os.name == "nt", "fixed POSIX shell fixture")
    def test_real_shell_preserves_numeric_entry_and_quoted_target_boundaries(self):
        path = "tests/test_lexical.py"
        root, _ = self.build("", [path])
        (root / "2").write_text("print('OTHER_NUMERIC_ENTRY')\n", encoding="utf-8")
        (root / path).write_text(
            "import unittest\nclass Cases(unittest.TestCase):\n"
            "    def test_failure(self): self.fail('LEXICAL_FAILURE_RAN')\n"
            "    def test_pass(self): pass\n"
            "if __name__ == '__main__': unittest.main()\n", encoding="utf-8")
        outputs = ("numeric.output", "stderr.output", "&", ";", ">", "out", "errors")
        for arguments, count, registered in (
            ("2 > numeric.output " + path, 0, False),
            ("'2'> numeric.output " + path, 0, False),
            ('"2"> numeric.output ' + path, 0, False),
            (r"\2> numeric.output " + path, 0, False),
            ('2""> numeric.output ' + path, 0, False),
            ("2> stderr.output " + path, 2, True),
            (path + " > '&'", 2, True),
            (path + r" > \&", 2, True),
            (path + " > ';' -k test_pass", 1, False),
            (path + " > '&' -k test_pass", 1, False),
            (path + " -k > '>' test_pass", 1, False),
            (path + " > out 2>&1", 2, True),
            ("2> errors " + path + " -k test_pass", 1, False),
        ):
            with self.subTest(arguments=arguments):
                for output in outputs:
                    (root / output).unlink(missing_ok=True)
                command = shlex.quote(sys.executable) + " -B " + arguments
                actual = subprocess.run(["/bin/sh", "-c", command], cwd=root,
                                        capture_output=True, text=True, timeout=15)
                text = actual.stdout + actual.stderr + "".join(
                    (root / output).read_text() for output in outputs if (root / output).exists())
                self.assertEqual(actual.returncode, 1 if count == 2 else 0, text)
                if count == 0:
                    self.assertIn("OTHER_NUMERIC_ENTRY", text)
                    self.assertNotIn("Ran ", text)
                else:
                    self.assertIn(f"Ran {count} test", text)
                self.assertEqual("LEXICAL_FAILURE_RAN" in text, count == 2)
                missing = guard.unregistered(root, "python3 -B " + arguments)
                self.assertEqual([rel for rel, _ in missing], [] if registered else [path])

    @unittest.skipUnless(os.path.isfile("/bin/bash"), "fixed Bash redirection fixture")
    def test_bash_combined_redirect_keeps_numeric_entry(self):
        path = "tests/test_combined.py"
        root, _ = self.build("", [path])
        (root / "2").write_text("print('COMBINED_NUMERIC_ENTRY')\n", encoding="utf-8")
        (root / path).write_text(
            "import unittest\nclass Cases(unittest.TestCase):\n"
            "    def test_failure(self): self.fail('COMBINED_FAILURE_RAN')\n"
            "    def test_pass(self): pass\n"
            "if __name__ == '__main__': unittest.main()\n", encoding="utf-8")
        for operator in ("&>", "&>>"):
            for prefix, options, count in (("2", "", 0), ("", "", 2), ("", " -k test_pass", 1)):
                with self.subTest(operator=operator, prefix=prefix, options=options):
                    output = root / "combined.log"
                    output.unlink(missing_ok=True)
                    arguments = prefix + operator + " combined.log " + path + options
                    actual = subprocess.run(
                        ["/bin/bash", "--noprofile", "--norc", "-c",
                         shlex.quote(sys.executable) + " -B " + arguments],
                        cwd=root, capture_output=True, text=True, timeout=15)
                    text = actual.stdout + actual.stderr + output.read_text()
                    self.assertEqual(actual.returncode, 1 if count == 2 else 0, text)
                    if count == 0:
                        self.assertIn("COMBINED_NUMERIC_ENTRY", text)
                        self.assertNotIn("Ran ", text)
                    else:
                        self.assertIn(f"Ran {count} test", text)
                    self.assertEqual("COMBINED_FAILURE_RAN" in text, count == 2)
                    missing = guard.unregistered(root, "python3 -B " + arguments)
                    self.assertEqual([rel for rel, _ in missing], [] if count == 2 else [path])

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

    def test_filtered_direct_script_exits_one_until_complete_call_exists(self):
        path = "tests/test_live.py"
        filtered = "python3 -B ./" + path + " -vk test_pass\n"
        self.assertEqual(self.run_main(filtered, [path]), 1)
        self.assertEqual(self.run_main(filtered + "python3 " + path + "\n", [path]), 0)

    def test_matching_orphan_outside_discovery_directory_exits_one(self):
        self.assertEqual(
            self.run_main("python3 -m unittest discover -s tools/fuzz -p 'test_*.py'\n",
                          ["tools/fuzz/test_control.py", "tools/test_orphan.py"]), 1)


    def test_start_directory_escaping_repo_root_is_rejected(self):
        self.assertEqual(self.run_main("python3 -m unittest discover -s .. -p 'test_*.py'\n", ["tools/test_x.py"]), 1)

if __name__ == "__main__":
    unittest.main()
