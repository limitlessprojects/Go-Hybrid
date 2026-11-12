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

Only explicit single-line Python invocations count. Workflow invocations
must run from the repository root, using step > job > workflow cwd defaults.
Block mappings and literal run blocks are supported; unknown cwd expressions
and unsupported mapping forms cannot establish root execution. Raw command
snippets retain their repository-root convention. Comments, echoed text,
step names, interpreter flags, chained later commands, and paths passed as
arguments to other scripts register nothing. This is not a general
shell/YAML interpreter.

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

# The workflow uses explicit Python commands, either in a run block or in a
# single-line run field. Do not count comments, step names, or echoed text
# as execution.
PYTHON_RE = re.compile(r"(?:.*/)?python(?:3(?:\.\d+)?)?\Z")
PYTHON_FLAGS_RE = re.compile(r"-[bBdEIOPqRsSuvx]+\Z")


def shell_words(line):
    lexer = shlex.shlex(line, posix=True, punctuation_chars=";&|<>()")
    lexer.whitespace_split = True
    return list(lexer)


def yaml_fields(lines, indent=None):
    """Read direct block-mapping fields, keeping scalar bodies opaque.

    This is deliberately a bounded reader, not a YAML loader. In particular,
    a run block's shell text is never searched for working-directory keys.
    """
    entries = []
    for index, line in enumerate(lines):
        match = re.match(r'''(?:"([^"]+)"|'([^']+)'|([\w-]+|<<))\s*:\s*(.*)$''',
                         line.strip())
        if match:
            key = next(value for value in match.groups()[:3] if value is not None)
            entries.append((len(line) - len(line.lstrip()), index, key, match[4]))
    if indent is None:
        indent = min((entry[0] for entry in entries), default=0)
    entries = [entry for entry in entries if entry[0] == indent]
    fields = {}
    for position, (_, index, key, value) in enumerate(entries):
        end = entries[position + 1][1] if position + 1 < len(entries) else len(lines)
        fields[key] = (value, lines[index + 1:end])
    return fields


def block_fields(node):
    """An inline/aliased/flow mapping is outside this reader's contract."""
    value, body = node
    return yaml_fields(body) if not value.split("#", 1)[0].strip() else None


def root_working_directory(value):
    quoted = re.fullmatch(r'''(['"])(.*?)\1(?:\s+#.*)?''', value.strip())
    scalar = (quoted[2] if quoted else re.split(r"\s+#", value.strip(), 1)[0])
    return scalar in (".", "./", "${{ github.workspace }}")


def default_root_cwd(fields, inherited):
    """Resolve defaults.run independently of mapping key order."""
    if "<<" in fields:
        return False  # A merge may provide unknown cwd metadata.
    defaults = fields.get("defaults")
    if defaults is None:
        return inherited
    defaults = block_fields(defaults)
    if defaults is None or "<<" in defaults:
        return False
    run = defaults.get("run")
    if run is None:
        return inherited
    run = block_fields(run)
    if run is None or "<<" in run:
        return False
    cwd = run.get("working-directory")
    return inherited if cwd is None else root_working_directory(cwd[0])


def workflow_run_lines(workflow_text):
    """Yield shell lines only from steps whose effective cwd is root.

    Literal block syntax avoids treating folded shell commands as independent
    lines. Flow mappings, aliases and unknown directories fail closed. Existing
    callers also pass raw command snippets rather than workflows; keep those
    as root snippets so the command parser remains independently usable.
    """
    lines = workflow_text.splitlines()
    workflow = yaml_fields(lines, indent=0)
    if "jobs" not in workflow:
        yield from lines
        return
    jobs = block_fields(workflow["jobs"])
    if jobs is None:
        return
    workflow_root = default_root_cwd(workflow, True)
    for job in jobs.values():
        fields = block_fields(job)
        if fields is None:
            continue
        job_root = default_root_cwd(fields, workflow_root)
        steps = fields.get("steps")
        if steps is None or steps[0].split("#", 1)[0].strip():
            continue
        body = steps[1]
        starts = [(len(line) - len(line.lstrip()), index)
                  for index, line in enumerate(body)
                  if re.match(r"-(?:\s|$)", line.lstrip())]
        if not starts:
            continue
        sequence_indent = min(indent for indent, _ in starts)
        starts = [index for indent, index in starts if indent == sequence_indent]
        for position, start in enumerate(starts):
            end = starts[position + 1] if position + 1 < len(starts) else len(body)
            step_lines = body[start:end].copy()
            # A sequence item is also the first key of its mapping. Normalize
            # that key's indentation, including indentless YAML sequences.
            step_lines[0] = " " * (sequence_indent + 2) + step_lines[0].lstrip()[1:].lstrip()
            step = yaml_fields(step_lines)
            cwd = step.get("working-directory")
            is_root = job_root if cwd is None else root_working_directory(cwd[0])
            run = step.get("run")
            if not is_root or run is None or "<<" in step:
                continue
            value, commands = run
            scalar = re.split(r"\s+#", value.strip(), 1)[0]
            if re.fullmatch(r"\|(?:[1-9][+-]?|[+-][1-9]?)?", scalar):
                yield from commands
            elif scalar.startswith(">"):
                continue
            elif not any(line.strip() and not line.lstrip().startswith("#")
                         for line in commands):
                yield "run: " + value


def python_commands(workflow_text):
    """Yield arguments of explicit, single-line Python invocations."""
    for line in workflow_run_lines(workflow_text):
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
            while arguments:
                option = arguments[0]
                if PYTHON_FLAGS_RE.fullmatch(option):
                    arguments = arguments[1:]
                elif option in ("-W", "-X", "--check-hash-based-pycs"):
                    arguments = arguments[2:]
                elif option.startswith(("-W", "-X")):
                    arguments = arguments[1:]
                elif option == "--":
                    arguments = arguments[1:]
                    break
                else:
                    break  # In particular, never interpret a -c string.
            yield arguments


# Directories whose test files this guard covers, and how each is invoked.
DISCOVER_DIR = "tools"   # referenced through a `unittest discover -p` pattern
DIRECT_DIRS = ("tests",)  # referenced by literal repo-relative path


def registered_discoveries(workflow_text):
    """Pair each discovery pattern with its own start directory (repo cwd).

    Like the original guard, this reads explicit one-line commands run from
    the repository root; it is not a general shell/YAML or load_tests
    interpreter.
    """
    discoveries = []
    for arguments in python_commands(workflow_text):
        if arguments[:3] != ["-m", "unittest", "discover"]:
            continue
        # argparse exits successfully for help before discovery imports any
        # test module. A help-only command cannot register a test suite.
        if any(argument in ("-h", "--help") for argument in arguments[3:]):
            continue
        start, pattern = ".", None
        for index, argument in enumerate(arguments[3:], 3):
            if argument in ("-s", "--start-directory") and index + 1 < len(arguments):
                start = arguments[index + 1]
            elif argument in ("-p", "--pattern") and index + 1 < len(arguments):
                pattern = arguments[index + 1]
            elif argument.startswith("--start-directory="):
                start = argument.split("=", 1)[1]
            elif argument.startswith("--pattern="):
                pattern = argument.split("=", 1)[1]
        if pattern is not None:
            discoveries.append((start, pattern))
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


def is_registered_direct(relpath, workflow_text):
    """A script must be the Python entry point, not a comment or argument."""
    return any(arguments and arguments[0] in (relpath, "./" + relpath)
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
              "`python3 -m unittest discover -s … -p …` for a discover suite (a glob "
              "like `test_job*.py` covers a family), or a direct "
              "`python3 <path>` invocation. Delete it instead if it is dead.")
        return 1

    print(f"test registration: OK ({len(found)} test file(s), "
          f"{len(patterns)} discovery pattern(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
