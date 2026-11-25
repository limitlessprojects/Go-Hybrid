#!/usr/bin/env python3
"""check_conflict_markers.py — fail closed on a corrupted workflow or a
committed merge-conflict marker.

Two failure classes live here, because they have the same shape: a line that
is syntactically invisible to every test suite but takes the whole pipeline
down.

1. A merge-conflict marker that reached a tracked file. Nothing imports a
   YAML workflow or a Markdown doc, so the suites never see it, but
   `.github/workflows/ci.yml` with a marker block is invalid YAML: the
   workflow never parses, no job is created, and every run reports failure
   with no logs — blocking every open pull request, because a PR is tested
   against its base.

2. A column-zero line inside `.github/workflows/*.{yml,yaml}` that is not a
   top-level key. Workflow files are a mapping at column zero, so a stray
   column-zero line is a parse error, and the most likely source is a
   duplicated shell line pasted out of a `run: |` block (the indentation is
   lost on the way through a tool round-trip, and the block scalar ends one
   line early). This is the same outage as (1) with different bytes.

Both classes have occurred on `main` for real.

For markers, only a real conflict BLOCK counts: a `<<<<<<<` line followed
later by a `>>>>>>>` line. A lone `=======` on its own is NOT reported,
because a setext heading underline or a horizontal rule is legitimate
Markdown and this repository uses them.

Stdlib only. Exit codes: 0 = clean, 1 = problems found, 2 = misuse.
"""
import argparse
import os
import re
import sys

START_RE = re.compile(r"^<<<<<<<(\s|$)")
END_RE = re.compile(r"^>>>>>>>(\s|$)")

# Directories that are never tracked content.
SKIP_DIRS = {".git", "target", "node_modules", ".venv", "__pycache__"}

# Extensions worth reading as text; anything else is skipped rather than
# guessed at, so a binary file cannot produce a spurious hit.
TEXT_EXTS = {
    ".md", ".yml", ".yaml", ".json", ".toml", ".rs", ".go", ".py", ".sh",
    ".s",  # Go bridge assembly (also covers uppercase .S via lower()).
    ".hy", ".txt", ".cfg", ".ini", ".html", ".css", ".js", ".ts",
}

# Files with no extension that still matter.
TEXT_NAMES = {"Makefile", "Dockerfile", "LICENSE", "NOTICE"}

# A workflow file is a mapping at column zero, so every column-zero line must
# be a key, a comment, or a document marker. Anything else is a parse error.
WORKFLOW_DIR = os.path.join(".github", "workflows")
WORKFLOW_EXTS = {".yml", ".yaml"}
WORKFLOW_EXT = ".yml"
TOP_LEVEL_KEY_RE = re.compile(
    r"^(?:[A-Za-z_][A-Za-z0-9_.\-]*|'[^'\n]+'|\"[^\"\n]+\")\s*:"
)


def is_text_candidate(path):
    name = os.path.basename(path)
    if name in TEXT_NAMES:
        return True
    return os.path.splitext(name)[1].lower() in TEXT_EXTS


def scan_file(path):
    """Return [(line_no, line)] for conflict blocks in one file.

    A block is a `<<<<<<<` line and the first following `>>>>>>>` line; the
    start line is reported (the block itself is the evidence).
    """
    hits = []
    open_start = None
    try:
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                stripped = line.rstrip("\n")
                if START_RE.match(stripped):
                    open_start = (n, stripped)
                elif END_RE.match(stripped) and open_start is not None:
                    hits.append(open_start)
                    open_start = None
        # An unterminated `<<<<<<<` is still a marker that must not ship.
        if open_start is not None:
            hits.append(open_start)
    except (UnicodeDecodeError, OSError):
        return []
    return hits


def scan_workflow_file(path):
    """Return [(line_no, line)] for column-zero lines that cannot be YAML.

    A workflow's top level is a mapping, so a non-blank line with zero
    indentation is only legal if it opens a key, is a comment, or is a
    document marker. Anything else — most often a shell line that fell out of
    a `run: |` block — makes the whole workflow unparseable, which is the
    failure this check exists to catch.
    """
    hits = []
    try:
        with open(path, encoding="utf-8") as f:
            for n, line in enumerate(f, 1):
                stripped = line.rstrip("\n")
                if not stripped.strip():
                    continue
                if stripped[0] in " \t":
                    continue
                if (
                    stripped.startswith("#")
                    or stripped.startswith("---")
                    or stripped.startswith("...")
                    or stripped.startswith("%")
                ):
                    continue
                if TOP_LEVEL_KEY_RE.match(stripped):
                    continue
                hits.append((n, stripped))
    except (UnicodeDecodeError, OSError):
        return []
    return hits


def _is_workflow(path):
    return (
        os.path.splitext(path)[1].lower() in WORKFLOW_EXTS
        and os.path.basename(os.path.dirname(path)) == "workflows"
        and os.path.basename(os.path.dirname(os.path.dirname(path))) == ".github"
    )


def scan(root):
    """Yield (relpath, line_no, line, kind) for every structural defect.

    `kind` is "marker" for a conflict block or "workflow" for a column-zero
    line that cannot parse as workflow YAML.
    """
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for fn in filenames:
            full = os.path.join(dirpath, fn)
            if is_text_candidate(full):
                for n, line in scan_file(full):
                    yield os.path.relpath(full, root), n, line, "marker"
            if _is_workflow(full):
                for n, line in scan_workflow_file(full):
                    yield os.path.relpath(full, root), n, line, "workflow"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".", help="tree to scan (default: cwd)")
    args = ap.parse_args()
    if not os.path.isdir(args.root):
        print(f"usage error: {args.root} is not a directory", file=sys.stderr)
        return 2
    found = list(scan(args.root))
    if not found:
        print("conflict markers: none")
        return 0

    markers = [h for h in found if h[3] == "marker"]
    broken = [h for h in found if h[3] == "workflow"]

    if markers:
        print(
            f"conflict markers: {len(markers)} block(s) found",
            file=sys.stderr,
        )
        for rel, n, line, _ in markers:
            print(f"  {rel}:{n}: {line}", file=sys.stderr)
        print(
            "A committed conflict marker means a merge was closed with the "
            "markers still in place. Remove the block before this can land.",
            file=sys.stderr,
        )
    if broken:
        print(
            f"workflow structure: {len(broken)} unparseable line(s) found",
            file=sys.stderr,
        )
        for rel, n, line, _ in broken:
            print(f"  {rel}:{n}: {line}", file=sys.stderr)
        print(
            "A workflow's top level is a mapping, so every line at column "
            "zero must open a key. This line cannot parse, which makes the "
            "whole workflow invalid and creates no jobs at all.",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    sys.exit(main())
