#!/usr/bin/env python3
"""Fail when tracked source-like files contain Git conflict markers."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Iterator

SKIP_DIRS = {".git", "target", "node_modules", ".venv", "__pycache__"}

# Restrict scanning to known text formats. This is deliberately an allowlist:
# unknown extensions are not guessed at, so a binary file cannot produce a
# spurious hit.
TEXT_EXTS = {
    ".md", ".yml", ".yaml", ".json", ".toml", ".rs", ".go", ".py", ".sh",
    ".s",  # Go bridge assembly (also covers uppercase .S via lower()).
    ".hy", ".txt", ".cfg", ".ini", ".html", ".css", ".js", ".ts",
}

MARKERS = ("<<<<<<<", "=======", ">>>>>>>")

Hit = tuple[Path, int, str]


def scan(root: str) -> Iterator[Hit]:
    """Yield (path, line_number, marker_line) for conflict-marker lines."""
    root_path = Path(root)

    for path in root_path.rglob("*"):
        if not path.is_file():
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.suffix.lower() not in TEXT_EXTS:
            continue

        try:
            with path.open("r", encoding="utf-8") as source:
                for line_number, line in enumerate(source, start=1):
                    if line.startswith(MARKERS):
                        yield path, line_number, line.rstrip("\n")
        except (OSError, UnicodeDecodeError):
            # A file classified as text but unreadable must not crash this
            # repository hygiene tool.
            continue


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check repository text files for Git conflict markers."
    )
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
        help="repository root to scan",
    )
    args = parser.parse_args(argv)

    hits = list(scan(args.root))
    if not hits:
        print("conflict markers: none")
        return 0

    for path, line_number, _line in hits:
        print(f"{path}:{line_number}: conflict marker")
    print(f"conflict markers: {len(hits)}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
