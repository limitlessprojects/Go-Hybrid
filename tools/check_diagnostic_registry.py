#!/usr/bin/env python3
"""Fail when a `.rs` source can print a `HYB-Exxxx` code the registry does not have.

The registry (`compiler/crates/hy-diag/data/diagnostics.json`,
SPEC-LANGUAGE.md §9.6) is declared append-only, never renumbered, never reused,
and it is what `hyb explain` serves. `tools/check_guarantees.py` polices one
direction of the join — a GUARANTEES row may not name a code that is missing
from the registry — but nothing policed the other direction, so the compiler
could print a code that has no registry entry and the only symptom was a
dead-end lookup:

    $ hyb check cl.hy
    error[HYB-E4004]: linear send capability 'tx' was not closed before scope exit
    $ hyb explain HYB-E4004
    unknown diagnostic code: HYB-E4004 (registry has 48 codes)   # exit 2

Both `HYB-E4004` and `HYB-E4005` shipped in that state (issue #4460): real
errors that fail a check, with no entry, so the user's normal next move — ask
what the code means and how to fix it — answers nothing.

Comment mentions count too: a doc comment that names a code is the same drift
one step earlier, and this tool's whole point is that the join stays closed
without anyone remembering to check.

Usage:
    python3 tools/check_diagnostic_registry.py [--root REPO_ROOT]

Exits 0 when every mentioned code is registered, 1 on drift (naming the code
and the `file:line` sites that mention it), 2 on harness misuse.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
REGISTRY_REL = Path("compiler") / "crates" / "hy-diag" / "data" / "diagnostics.json"
SRC_GLOB = ("compiler", "crates")
CODE_RE = re.compile(r"HYB-[EW]\d{4}")
MAX_SITES = 5


def registry_codes(registry_path: Path) -> set[str]:
    """The codes the registry actually carries. Raises on a malformed file."""
    data = json.loads(registry_path.read_text(encoding="utf-8"))
    codes = data.get("codes")
    if not isinstance(codes, list):
        raise ValueError(f"{registry_path}: no `codes` list")
    out: set[str] = set()
    for entry in codes:
        code = entry.get("code") if isinstance(entry, dict) else None
        if not isinstance(code, str):
            raise ValueError(f"{registry_path}: entry without a `code` string")
        out.add(code)
    return out


def mention_sites(root: Path) -> dict[str, list[str]]:
    """Every `HYB-Exxxx` mentioned by any crate's `src/`, with `path:line` sites."""
    crates = root.joinpath(*SRC_GLOB)
    sites: dict[str, list[str]] = {}
    for path in sorted(crates.glob("*/src/**/*.rs")):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        for lineno, line in enumerate(text.splitlines(), 1):
            for code in CODE_RE.findall(line):
                rel = path.relative_to(root).as_posix()
                sites.setdefault(code, []).append(f"{rel}:{lineno}")
    return sites


def unregistered(root: Path) -> tuple[dict[str, list[str]], int]:
    """(unregistered code -> sites, number of registry codes)."""
    codes = registry_codes(root / REGISTRY_REL)
    sites = mention_sites(root)
    return {c: s for c, s in sites.items() if c not in codes}, len(codes)


def main(argv: list[str]) -> int:
    root = REPO_ROOT
    args = list(argv)
    while args:
        arg = args.pop(0)
        if arg == "--root":
            if not args:
                print("check_diagnostic_registry: --root needs a path", file=sys.stderr)
                return 2
            root = Path(args.pop(0)).resolve()
        elif arg in ("-h", "--help"):
            print(__doc__)
            return 0
        else:
            print(f"check_diagnostic_registry: unknown argument {arg!r}", file=sys.stderr)
            return 2

    registry_path = root / REGISTRY_REL
    if not registry_path.is_file():
        print(f"check_diagnostic_registry: no registry at {registry_path}", file=sys.stderr)
        return 2
    if not root.joinpath(*SRC_GLOB).is_dir():
        print(f"check_diagnostic_registry: no crates under {root}", file=sys.stderr)
        return 2

    try:
        missing, total = unregistered(root)
    except (ValueError, OSError) as exc:
        print(f"check_diagnostic_registry: malformed registry: {exc}", file=sys.stderr)
        return 2
    if missing:
        print(
            f"DRIFT: {len(missing)} code(s) can be printed by the compiler but are "
            f"not in the registry ({total} registered):",
            file=sys.stderr,
        )
        for code in sorted(missing):
            sites = missing[code]
            print(f"  {code}  ({len(sites)} mention(s))", file=sys.stderr)
            for site in sites[:MAX_SITES]:
                print(f"      {site}", file=sys.stderr)
            if len(sites) > MAX_SITES:
                print(f"      … {len(sites) - MAX_SITES} more", file=sys.stderr)
        print(
            "\nRegister each code in "
            f"{REGISTRY_REL.as_posix()} (ascending order, never renumbered or reused), "
            "with a name, title, summary, fix-its and an example, so `hyb explain` serves it.",
            file=sys.stderr,
        )
        return 1

    print(f"OK: every code mentioned under compiler/crates/*/src/ is registered ({total} codes).")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
