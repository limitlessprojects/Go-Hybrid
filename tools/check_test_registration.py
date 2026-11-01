#!/usr/bin/env python3
"""check_test_registration.py — every committed test file must be run by CI.

A merged regression test that nothing invokes cannot fail the build: it
reads as a passing test while pinning nothing. Four suites had drifted out
of .github/workflows/ci.yml that way (test_job_cli, test_job_payload,
test_corpus_error_diagnostics, test_differential_verdict_markers) — each
landed in a commit that never touched the workflow, so every green check
since meant less than it looked.

This guard resolves each test file the way the workflow would have to
reference it, and fails closed on any file no reference reaches:

  tools/**/test_*.py  run by `python3 -m unittest discover -s DIR -p PATTERN`
                      -> the file must lie inside DIR, be reachable through
                         package directories, and match PATTERN (a glob such
                         as `test_job*.py` covers a family)
  tests/**/test_*.py  run directly, e.g. `python3 tests/test_install_runtime.py`
                      -> the path must be the invoked Python entry point

Only explicit single-line Python invocations count. Comments, echoed text,
step names, interpreter flags, chained later commands, and paths passed as
arguments to other scripts register nothing. This is not a general
shell/YAML interpreter.

Usage: python3 tools/check_test_registration.py [--workflow PATH] [--root DIR]
Exit:  0 = every test file is registered; 1 = unregistered file(s); 2 = guard misuse
       (no workflow or no test files found, an unreadable/undecodable --workflow,
       or a --root that is not a directory).
"""

import argparse
import fnmatch
import os
from pathlib import Path
import posixpath
import re
import shlex
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# What `unittest discover` itself uses when a command omits `-p`: a discovery
# without a pattern still registers the files it reaches.
DEFAULT_DISCOVER_PATTERN = "test*.py"

# The workflow uses explicit Python commands, either in a run block or in a
# single-line run field. Do not count comments, step names, or echoed text
# as execution.
PYTHON_RE = re.compile(r"(?:.*/)?python(?:3(?:\.\d+)?)?\Z")
PYTHON_FLAGS_RE = re.compile(r"-[bBdEIOPqRsSuvx]+\Z")


def shell_words(line):
    lexer = shlex.shlex(line, posix=True, punctuation_chars=";&|<>()")
    lexer.whitespace_split = True
    return list(lexer)


def python_commands(workflow_text):
    """Yield arguments of explicit, single-line Python invocations."""
    for line in workflow_text.splitlines():
        line = line.strip()
        inline_run = re.match(r"(?:-\s+)?run:\s*(.*)", line)
        if inline_run:
            line = inline_run.group(1)
        try:
            tokens = shell_words(line)
            # A quoted YAML run scalar wraps the whole shell command.
            if inline_run and len(tokens) == 1:
                tokens = shell_words(tokens[0])
        except ValueError:
            continue
        if tokens and PYTHON_RE.fullmatch(tokens[0]):
            # Only the first simple command is covered. A following echo's
            # -p flag must never become another unittest discovery pattern.
            end = next((i for i, token in enumerate(tokens)
                        if token and all(c in ";&|<>()" for c in token)), len(tokens))
            # Issue #2880: an OR-list (`||`), pipeline (`|`), or background
            # operator (`&`) masks a non-zero exit status under `set -e`
            # (without `pipefail`), so a failing test suite would not fail CI.
            if any(
                token and all(c in ";&|<>()" for c in token)
                and ("|" in token or "&" in token.replace("&&", ""))
                for token in tokens[end:]
            ):
                continue
            arguments = tokens[1:end]
            malformed = False
            while arguments:
                option = arguments[0]
                if PYTHON_FLAGS_RE.fullmatch(option):
                    arguments = arguments[1:]
                elif option in ("-W", "-X", "--check-hash-based-pycs"):
                    # These three take a value, and a second flag (or nothing)
                    # is not one: `python3 -W -u tests/test_live.py` is an
                    # interpreter error, so it must not register a test.
                    if len(arguments) < 2 or arguments[1].startswith("-"):
                        malformed = True
                        break
                    arguments = arguments[2:]
                elif option.startswith(("-W", "-X")) or \
                        option.startswith("--check-hash-based-pycs="):
                    arguments = arguments[1:]  # value carried inline
                elif option == "--":
                    arguments = arguments[1:]
                    break
                else:
                    break  # In particular, never interpret a -c string.
            if not malformed:
                yield arguments


# Directories whose test files this guard covers, and how each is invoked.
DISCOVER_DIR = "tools"   # referenced through a `unittest discover -p` pattern
DIRECT_DIRS = ("tests",)  # referenced by literal repo-relative path


def option_value(arguments, index):
    """The value token of a two-token option, or None when there is none.

    argparse refuses to read a following flag as an option value, so
    `discover -s -p test_foo.py` is a malformed command rather than a start
    directory of `-p` — and it must register nothing (issue #1333).
    """
    if index + 1 >= len(arguments):
        return None
    value = arguments[index + 1]
    if not value or value.startswith("-"):
        return None
    return value


def registered_discoveries(workflow_text):
    """Pair each discovery pattern with its own start directory (repo cwd).

    Like the original guard, this reads explicit one-line commands run from
    the repository root; it is not a general shell/YAML or load_tests
    interpreter. Each consumed option value is skipped instead of re-read as
    another option, a discovery without an explicit `-p` keeps unittest's own
    default pattern, and a `-s`/`-p` whose value is missing or flag-shaped
    registers nothing.
    """
    discoveries = []
    for arguments in python_commands(workflow_text):
        if arguments[:3] != ["-m", "unittest", "discover"]:
            continue
        start, pattern = ".", None
        index = 3
        malformed = False
        while index < len(arguments):
            argument = arguments[index]
            if argument in ("-s", "--start-directory", "-p", "--pattern"):
                value = option_value(arguments, index)
                if value is None:
                    malformed = True
                    break
                if argument in ("-s", "--start-directory"):
                    start = value
                else:
                    pattern = value
                index += 2
            elif argument.startswith("--start-directory="):
                start = argument.split("=", 1)[1]
                index += 1
            elif argument.startswith("--pattern="):
                pattern = argument.split("=", 1)[1]
                index += 1
            elif argument in ("-t", "--top-level-directory", "--directory"):
                index += 2  # takes a value; the value registers nothing itself
            elif argument.startswith(("--top-level-directory=", "--directory=")):
                index += 1
            else:
                index += 1  # `-v` and friends: never a value, never an option
        if not malformed:
            discoveries.append(
                (start, DEFAULT_DISCOVER_PATTERN if pattern is None else pattern))
    return discoveries


def registered_patterns(workflow_text):
    """Every `-p` pattern the workflow passes to `unittest discover`."""
    return [pattern for _, pattern in registered_discoveries(workflow_text)]


def is_registered_discover(name, patterns):
    return any(fnmatch.fnmatch(name, pattern) for pattern in patterns)


def is_reached_discover(root, relpath, start, pattern):
    """Mirror discovery's directory boundary without importing test code."""
    root_dir = Path(root).resolve()
    target = (root_dir / relpath).resolve()
    start_dir = (root_dir / start).resolve()
    if not start_dir.is_dir() or not start_dir.is_relative_to(root_dir) or not target.is_relative_to(start_dir):
        return False
    if not is_registered_discover(target.name, [pattern]):
        return False
    # unittest visits files in the start directory directly, but only
    # recurses into subdirectories that are regular Python packages.
    parent = target.parent
    while parent != start_dir:
        if not (parent / "__init__.py").is_file():
            return False
        parent = parent.parent
    return True


def script_path(path):
    """Normalize a repo-relative script path, or None when it names none.

    An interpreter token (`-m`, `-c`, `-W`) is never a script, and a path with
    a `..` segment can address a file outside the directories this guard
    covers, so neither counts as an invocation. Equivalent spellings of the
    same path (`./tests/x.py`, `tests//x.py`) do (#1333).
    """
    if not path or path.startswith("-"):
        return None
    if any(segment == ".." for segment in path.split("/")):
        return None
    return posixpath.normpath(path)


def is_registered_direct(relpath, workflow_text):
    """A script must be the Python entry point, not a comment or argument."""
    target = script_path(relpath)
    if target is None:
        return False
    return any(arguments and script_path(arguments[0]) == target
               for arguments in python_commands(workflow_text))


def find_test_files(root, directory):
    """Repo-relative paths of every test_*.py under root/directory."""
    found = []
    base_dir = os.path.join(root, directory)
    for base, _dirs, files in os.walk(base_dir):
        for filename in files:
            if filename.startswith("test_") and filename.endswith(".py"):
                rel = os.path.relpath(os.path.join(base, filename), root)
                found.append(rel.replace(os.sep, "/"))
    return sorted(found)


def unregistered(root, workflow_text):
    """Test files no reference in the workflow reaches."""
    discoveries = registered_discoveries(workflow_text)
    missing = []
    for rel in find_test_files(root, DISCOVER_DIR):
        if not any(is_reached_discover(root, rel, start, pattern)
                   for start, pattern in discoveries):
            missing.append((rel, "no `unittest discover -s ... -p ...` invocation reaches it"))
    for directory in DIRECT_DIRS:
        for rel in find_test_files(root, directory):
            if not is_registered_direct(rel, workflow_text):
                missing.append((rel, "its path is never invoked by the workflow"))
    return missing


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--workflow",
                    default=os.path.join(REPO_ROOT, ".github", "workflows", "ci.yml"),
                    help="workflow whose references define registration")
    ap.add_argument("--root", default=REPO_ROOT,
                    help="repository root holding the test directories")
    args = ap.parse_args()

    # Misuse is exit 2, not a traceback: an unreadable workflow or a --root
    # that is not a directory says nothing about the repository's tests (#1333).
    if not os.path.isdir(args.root):
        print(f"REGISTRATION CHECK ERROR: --root {args.root} is not a directory",
              file=sys.stderr)
        return 2
    try:
        with open(args.workflow, encoding="utf-8") as handle:
            workflow_text = handle.read()
    except (OSError, UnicodeDecodeError) as error:
        print(f"REGISTRATION CHECK ERROR: cannot read --workflow {args.workflow}: "
              f"{error}", file=sys.stderr)
        return 2

    patterns = registered_patterns(workflow_text)
    found = (find_test_files(args.root, DISCOVER_DIR)
             + [t for d in DIRECT_DIRS for t in find_test_files(args.root, d)])
    if not found:
        print(f"REGISTRATION CHECK ERROR: no test_*.py found under "
              f"{args.root} ({DISCOVER_DIR}/ or {', '.join(DIRECT_DIRS)}/)",
              file=sys.stderr)
        return 2
    if not patterns and not any(d in workflow_text for d in DIRECT_DIRS):
        print(f"REGISTRATION CHECK ERROR: {args.workflow} references no test at "
              "all; the guard cannot verify anything", file=sys.stderr)
        return 2

    missing = unregistered(args.root, workflow_text)
    if missing:
        print(f"UNREGISTERED TEST FILE(S) — nothing runs these, so they cannot fail CI:")
        for rel, why in missing:
            print(f"  {rel} — {why}")
        print(f"Register each in {args.workflow}: a variant of "
              "`python3 -m unittest discover -s … -p …` for a discover suite (a glob "
              "like `test_job*.py` covers a family), or a direct "
              "`python3 <path>` invocation. Delete it instead if it is dead.")
        return 1

    print(f"test registration: OK ({len(found)} test file(s), "
          f"{len(patterns)} discovery pattern(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
