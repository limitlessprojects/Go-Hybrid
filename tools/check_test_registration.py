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
                      -> the file must be reachable from DIR and match PATTERN
                         (a glob such as `test_job*.py` covers a family)
  tests/**/test_*.py  run directly, e.g. `python3 tests/test_install_runtime.py`
                      -> the repo-relative path must appear in the workflow

Usage: python3 tools/check_test_registration.py [--workflow PATH] [--root DIR]
Exit:  0 = every test file is registered; 1 = unregistered file(s); 2 = guard misuse.
"""

import argparse
import fnmatch
import os
from pathlib import Path
import re
import shlex
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# One discovery invocation, quoted or bare pattern: `... discover -s X -p Y`.
DISCOVER_RE = re.compile(r"unittest\s+discover\b([^\n]*)")

# Directories whose test files this guard covers, and how each is invoked.
DISCOVER_DIR = "tools"   # referenced through a `unittest discover -p` pattern
DIRECT_DIRS = ("tests",)  # referenced by literal repo-relative path


def registered_discoveries(workflow_text):
    """Pair each explicit discovery pattern with its own start directory.

    Like the original guard, this reads single-line discovery commands with
    explicit patterns. Paths are relative to the repository root; this is not
    a shell/YAML interpreter or an evaluator of custom load_tests hooks.
    """
    discoveries = []
    for match in DISCOVER_RE.finditer(workflow_text):
        try:
            arguments = shlex.split(match.group(1), comments=True)
        except ValueError:
            continue  # An unparseable reference cannot register a suite.
        start, pattern = ".", None
        arguments = iter(arguments)
        for argument in arguments:
            if argument in ("-s", "--start-directory"):
                start = next(arguments, "")
            elif argument in ("-p", "--pattern"):
                pattern = next(arguments, "")
            elif argument.startswith("--start-directory="):
                start = argument.split("=", 1)[1]
            elif argument.startswith("--pattern="):
                pattern = argument.split("=", 1)[1]
        if start and pattern:
            discoveries.append((start, pattern))
    return discoveries


def registered_patterns(workflow_text):
    """Every explicit pattern the workflow passes to `unittest discover`."""
    return [pattern for _, pattern in registered_discoveries(workflow_text)]


def is_registered_discover(relpath, discoveries, root):
    target = (Path(root) / relpath).resolve()
    for directory, pattern in discoveries:
        if not fnmatch.fnmatch(target.name, pattern):
            continue
        start = (Path(root) / directory).resolve()
        if not target.is_relative_to(start):
            continue
        parent = target.parent
        # unittest descends through packages, not arbitrary subdirectories.
        # The explicitly selected start directory itself needs no __init__.py.
        while parent != start and (parent / "__init__.py").is_file():
            parent = parent.parent
        if parent == start:
            return True
    return False


def is_registered_direct(relpath, workflow_text):
    """A directly-invoked script is registered when its path is named."""
    return re.search(r"(?<![\w./-])" + re.escape(relpath) + r"(?![\w.-])",
                     workflow_text) is not None


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
        if not is_registered_discover(rel, discoveries, root):
            missing.append((rel, "no `unittest discover` directory/pattern reaches it"))
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

    with open(args.workflow, encoding="utf-8") as handle:
        workflow_text = handle.read()

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
              "`python3 -m unittest discover -p …` for a discover suite (a glob "
              "like `test_job*.py` covers a family), or a direct "
              "`python3 <path>` invocation. Delete it instead if it is dead.")
        return 1

    print(f"test registration: OK ({len(found)} test file(s), "
          f"{len(patterns)} discovery pattern(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
