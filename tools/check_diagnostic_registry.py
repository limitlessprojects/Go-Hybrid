#!/usr/bin/env python3
"""Fail when the compiler can print a diagnostic code the registry does not know.

`compiler/crates/hy-diag/data/diagnostics.json` is the append-only code
registry (SPEC-LANGUAGE.md §9.6): it is what `hyb explain` serves, and it is the
join key every GUARANTEES row cites. Nothing enforced the join from the other
side, so a code could be emitted and simply never be registered — and two were:
`HYB-E4004` and `HYB-E4005` (the linear-capability terminal invariants in
`hy-check/src/channel_liveness.rs`). A user got

    error[HYB-E4004]: linear send capability 'tx' was not closed before scope exit

and `hyb explain HYB-E4004` answered `unknown diagnostic code: HYB-E4004
(registry has 48 codes)`. A GUARANTEES row could not cite either code either,
because `check_guarantees.py` fails on a row naming an unregistered code.

The direction of the check: every `HYB-E####` a `.rs` source can PRINTS must be
in the registry. Comment mentions count too — a doc comment naming a code that
does not exist is the same drift one step earlier, and it is cheap to keep
honest (the whole set is 50 codes).

Usage:
    python3 tools/check_diagnostic_registry.py

Exit: 0 = every emitted code is registered; 1 = unregistered code(s); 2 = misuse.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REGISTRY_REL = Path("compiler") / "crates" / "hy-diag" / "data" / "diagnostics.json"
SOURCE_GLOB = "compiler/crates/*/src/**/*.rs"

CODE_RE = re.compile(r"HYB-E\d{4}")


def registry_codes(root: Path) -> set[str]:
    data = json.loads((root / REGISTRY_REL).read_text(encoding="utf-8"))
    return {entry["code"] for entry in data["codes"]}


def emitted(root: Path) -> list[tuple[str, int, str]]:
    """(file, line, code) for every code occurrence in a compiler source tree."""
    found: list[tuple[str, int, str]] = []
    for path in sorted(root.glob(SOURCE_GLOB)):
        rel = path.relative_to(root).as_posix()
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            for match in CODE_RE.finditer(line):
                found.append((rel, lineno, match.group(0)))
    return found


def collect_problems(root: Path) -> list[str]:
    registry = root / REGISTRY_REL
    if not registry.exists():
        return [f"no registry at {REGISTRY_REL} — nothing to join against"]
    known = registry_codes(root)
    if not known:
        return [f"{REGISTRY_REL} registers no codes; refusing to certify"]

    problems = []
    for rel, lineno, code in emitted(root):
        if code not in known:
            problems.append(f"{rel}:{lineno}: {code} is emitted but not in {REGISTRY_REL}")
    return problems


def main(argv: list[str]) -> int:
    root = Path(argv[1]).resolve() if len(argv) > 1 else REPO_ROOT
    if not (root / "compiler").is_dir():
        print(f"ERROR: no compiler/ under {root}", file=sys.stderr)
        return 2

    problems = collect_problems(root)
    if problems:
        print("ERROR: diagnostic registry drift — the compiler prints unregistered codes:")
        for problem in problems:
            print(f"  - {problem}")
        print(
            "\nRegister each code in "
            f"{REGISTRY_REL.as_posix()} (append-only, ascending code order: "
            "code, name, category, severity, title, summary, fixits, example) "
            "so `hyb explain` can serve it and a GUARANTEES row can cite it."
        )
        return 1

    print(f"OK: every emitted diagnostic code is registered ({len(registry_codes(root))} codes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
