#!/usr/bin/env python3
"""Fail when a corpus count claim disagrees with the corpus on disk (issue #389 class).

`tests/checker-corpus/` is the spec, and several files restate its size:
`parse_all.rs` asserts it, `ci.yml` and the `Makefile` name it in a step
label, and `README.md` / `GUARANTEES.md` / `ast.rs` describe it in prose.
Every corpus PR has to move those numbers, so they drift apart whenever one
is missed — and a stale count is not cosmetic: it is a claim about the spec
that the spec contradicts.

This happened at least twice. PR #389 was opened to "repair VERDICTS.md row
drift, running totals" and landed; the same drift was present again on
session-22's main (Makefile / README.md / GUARANTEES.md said 60, ast.rs 63,
the ci.yml label and the corpus README 77, and the corpus held 80).

The count is derived here from the corpus itself, never from another claim.

STATUS.md is deliberately not policed: it is a supervisor-only file in this
pipeline (agents may not edit it), and the supervisor refreshes it per
landing.

Usage:
    python3 tools/check_corpus_census.py

Exits 0 when every surface agrees with the corpus, 1 otherwise (naming each
stale surface and the number it should carry), 2 on harness misuse.
"""

from __future__ import annotations

import re
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CORPUS_REL = Path("tests") / "checker-corpus"

VERDICT_RE = re.compile(
    r"^//\s*EXPECTED-VERDICT:\s*(ACCEPT-MANAGED|ACCEPT|REJECT)\b", re.M
)

# (path, regex with capture group(s) holding the claimed total, what it is)
TOTAL_SURFACES: list[tuple[str, str, str]] = [
    (
        "compiler/crates/hy-syntax/tests/parse_all.rs",
        # anchored on the corpus wording: the same file also asserts the
        # spike-6 count with the identical `files.len(), N,` shape.
        r"files\.len\(\),\s*(\d+),\s*\"expected exactly (\d+) corpus programs\"",
        "the corpus parse-all assertion",
    ),
    (
        ".github/workflows/ci.yml",
        r"verdict agreement \((\d+)/(\d+) \+ tripwire\)",
        "the corpus CI step label",
    ),
    (
        "Makefile",
        r"corpus verdicts \((\d+)/(\d+)\)",
        "the `make corpus` help line",
    ),
    (
        "README.md",
        r"checker corpus (\d+)/(\d+)",
        "the README corpus claim",
    ),
    (
        "GUARANTEES.md",
        r"^\s*(\d+) programs, each with an",
        "the GUARANTEES evidence-inventory line",
    ),
    (
        "compiler/crates/hy-syntax/src/ast.rs",
        r"checker-corpus/\*\.hy \((\d+) programs\)",
        "the ast.rs scope note",
    ),
]

# the corpus README also restates the total and the verdict breakdown
README_BREAKDOWN = (
    "tests/checker-corpus/README.md",
    r"corpus:\s*(\d+) programs\D+ACCEPT (\d+)\D+ACCEPT-MANAGED (\d+)\D+REJECT (\d+)",
)


def census(corpus_dir: Path) -> tuple[int, Counter]:
    """The truth: derived from the corpus files, never from another claim."""
    files = sorted(corpus_dir.glob("*.hy"))
    counts: Counter = Counter()
    for path in files:
        text = path.read_text(encoding="utf-8")
        match = VERDICT_RE.search(text)
        counts[match.group(1) if match else "NO-VERDICT"] += 1
    return len(files), counts


def check_total_surfaces(root: Path, total: int) -> list[str]:
    problems: list[str] = []
    for rel, pattern, label in TOTAL_SURFACES:
        path = root / rel
        if not path.exists():
            problems.append(f"{rel}: missing ({label})")
            continue
        text = path.read_text(encoding="utf-8")
        matches = re.findall(pattern, text, re.M)
        if not matches:
            problems.append(
                f"{rel}: no corpus count found for {label} — "
                f"the pattern {pattern!r} no longer matches; update this guard"
            )
            continue
        for match in matches:
            groups = match if isinstance(match, tuple) else (match,)
            seen_in_match: set[int] = set()
            for claim in groups:
                val = int(claim)
                if val != total and val not in seen_in_match:
                    seen_in_match.add(val)
                    problems.append(
                        f"{rel}: says {claim}, corpus holds {total} ({label})"
                    )
    return problems


def check_breakdown(root: Path, total: int, counts: Counter) -> list[str]:
    rel, pattern = README_BREAKDOWN
    path = root / rel
    if not path.exists():
        return [f"{rel}: missing"]
    match = re.search(pattern, path.read_text(encoding="utf-8"), re.M)
    if not match:
        return [
            f"{rel}: no verdict breakdown found — the pattern no longer matches; "
            f"update this guard"
        ]
    claimed_total, accept, managed, reject = (int(g) for g in match.groups())
    problems: list[str] = []
    if claimed_total != total:
        problems.append(f"{rel}: says {claimed_total} programs, corpus holds {total}")
    if (accept, managed, reject) != (
        counts["ACCEPT"],
        counts["ACCEPT-MANAGED"],
        counts["REJECT"],
    ):
        problems.append(
            f"{rel}: breakdown says ACCEPT {accept} / ACCEPT-MANAGED {managed} / "
            f"REJECT {reject}, corpus holds ACCEPT {counts['ACCEPT']} / "
            f"ACCEPT-MANAGED {counts['ACCEPT-MANAGED']} / REJECT {counts['REJECT']}"
        )
    return problems


def collect_problems(root: Path) -> tuple[list[str], int, Counter]:
    """Every disagreement with the corpus, plus the derived census."""
    corpus_dir = root / CORPUS_REL
    if not corpus_dir.is_dir():
        return [f"no corpus directory at {corpus_dir}"], 0, Counter()

    total, counts = census(corpus_dir)
    if total == 0:
        return ["the corpus is empty; refusing to certify a count"], 0, counts

    problems = check_total_surfaces(root, total) + check_breakdown(root, total, counts)
    no_verdict = counts.get("NO-VERDICT", 0)
    if no_verdict:
        problems.append(
            f"{no_verdict} corpus file(s) carry no EXPECTED-VERDICT line, so the "
            f"breakdown cannot be trusted"
        )
    return problems, total, counts


def main() -> int:
    problems, total, counts = collect_problems(REPO_ROOT)

    if problems and total == 0:
        print(f"ERROR: {problems[0]}", file=sys.stderr)
        return 2

    if problems:
        print("ERROR: corpus census drift — a claim disagrees with the corpus:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        print(
            f"\nThe corpus holds {total} programs: ACCEPT {counts['ACCEPT']} · "
            f"ACCEPT-MANAGED {counts['ACCEPT-MANAGED']} · REJECT {counts['REJECT']}.",
            file=sys.stderr,
        )
        print("Update each surface above to that number.", file=sys.stderr)
        return 1

    print(
        f"OK: all {len(TOTAL_SURFACES) + 1} corpus census surfaces agree with the "
        f"corpus ({total} programs: ACCEPT {counts['ACCEPT']} · "
        f"ACCEPT-MANAGED {counts['ACCEPT-MANAGED']} · REJECT {counts['REJECT']})."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
