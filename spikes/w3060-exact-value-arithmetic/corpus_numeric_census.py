"""Spike W-3060 — corpus numeric census (M1 item 2 reconnaissance, #3060).

**Exploratory, not pre-registered.**  The two pre-registered hypotheses live in
`PREREGISTRATION.md`; this scan was run *before* it was written up, so it is
reported as reconnaissance and produces an **amendment** to the wildcard's M1
item 2 (see `CENSUS.md`), never a "prediction confirmed" claim.

It answers one question the 50% static-discharge defeat condition in #3060
depends on: **does the 130-program corpus contain enough typed, narrow-width
arithmetic to measure a discharge rate at all?**

The scan is lexical (regex over the `.hy` text, no AST) and says so; it is a
sizing scan, not a soundness analysis.

    python corpus_numeric_census.py [corpus-dir]
"""
from __future__ import annotations

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_CORPUS = os.path.normpath(os.path.join(HERE, "..", "..", "tests", "checker-corpus"))

WIDTH = {
    "i8": 8, "i16": 16, "i32": 32, "i64": 64, "isize": 64,
    "u8": 8, "u16": 16, "u32": 32, "u64": 64, "usize": 64,
    "int": 64, "uint": 64, "uintptr": 64,
    "int8": 8, "int16": 16, "int32": 32, "int64": 64,
    "uint8": 8, "uint16": 16, "uint32": 32, "uint64": 64,
    "byte": 8, "rune": 32, "float32": 32, "float64": 64, "f32": 32, "f64": 64,
    "bool": 0, "string": 0,
}
NUMERIC = "|".join(WIDTH)
DECL = re.compile(rf"\b(?:var|let|const)\s+([a-z_][a-z0-9_]*)\s+({NUMERIC})\b")
BINOP = re.compile(r"[a-zA-Z0-9_)\]]\s*(\+|-|\*|/|%|<<|>>|&|\||\^)(?!=)\s*[a-zA-Z0-9_(]")
NAME_OP_NAME = re.compile(r"\b([a-z_][a-z0-9_]*)\s*(\+|-|\*|/|%|<<|>>|&|\||\^)\s*([a-z_][a-z0-9_]*)\b")


def census(directory=DEFAULT_CORPUS):
    files = sorted(f for f in os.listdir(directory) if f.endswith(".hy"))
    report = {
        "files": len(files), "declarations": 0, "narrow_declarations": 0,
        "files_with_any_declaration": 0, "files_with_narrow_declaration": 0,
        "operator_occurrences": 0, "files_with_operators": 0,
        "sites_both_declared": 0, "sites_both_declared_narrow": 0,
        "sites_width_mismatch": 0, "per_type": {}, "examples": [],
    }
    for name in files:
        with open(os.path.join(directory, name), encoding="utf-8") as handle:
            src = handle.read()
        typing = {}
        for decl in DECL.finditer(src):
            ident, ty = decl.group(1), decl.group(2)
            typing[ident] = ty
            report["declarations"] += 1
            report["per_type"][ty] = report["per_type"].get(ty, 0) + 1
            if 0 < WIDTH[ty] < 64:
                report["narrow_declarations"] += 1
        if typing:
            report["files_with_any_declaration"] += 1
        if any(0 < WIDTH[t] < 64 for t in typing.values()):
            report["files_with_narrow_declaration"] += 1

        ops = list(BINOP.finditer(src))
        report["operator_occurrences"] += len(ops)
        if ops:
            report["files_with_operators"] += 1

        for site in NAME_OP_NAME.finditer(src):
            a, op, b = site.group(1), site.group(2), site.group(3)
            ta, tb = typing.get(a), typing.get(b)
            if ta is None or tb is None:
                continue
            report["sites_both_declared"] += 1
            if WIDTH[ta] != WIDTH[tb]:
                report["sites_width_mismatch"] += 1
                note = "width-mismatch"
            elif 0 < WIDTH[ta] < 64:
                report["sites_both_declared_narrow"] += 1
                note = "narrow"
            else:
                continue
            if len(report["examples"]) < 6:
                report["examples"].append(f"[{note}] {name}: {a}({ta}) {op} {b}({tb})")
    return report


def render(report):
    lines = [
        f"corpus files: {report['files']}",
        "",
        "== declarations ==",
        f"typed numeric declarations: {report['declarations']}",
        f"  sub-word width (<64 bit, incl. byte/rune/float32): {report['narrow_declarations']}",
        f"files with any typed numeric declaration: {report['files_with_any_declaration']} / {report['files']}",
        f"files with >=1 sub-word-width declaration: {report['files_with_narrow_declaration']} / {report['files']}",
        "",
        "== arithmetic surface (lexical) ==",
        f"binary arithmetic operator occurrences: {report['operator_occurrences']}",
        f"files containing >=1 such operator: {report['files_with_operators']} / {report['files']}",
        "",
        "== sites where BOTH operand names have a declared numeric type ==",
        f"total: {report['sites_both_declared']}",
        f"  same width, sub-word: {report['sites_both_declared_narrow']}",
        f"  DIFFERENT width (a Go build error): {report['sites_width_mismatch']}",
        "",
        "per-type declaration counts:",
    ]
    for ty, count in sorted(report["per_type"].items()):
        lines.append(f"  {ty}: {count}")
    lines += ["", "examples (name(declared type) op name(declared type)):"]
    lines += [f"  {e}" for e in report["examples"]]
    return "\n".join(lines)


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    directory = argv[0] if argv else DEFAULT_CORPUS
    print(render(census(directory)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
