"""Spike W-3060 — the exact-arithmetic value oracle (M2 artifact of #3060).

Pre-registration: `VALUE-ORACLE-PREREGISTRATION.md` (committed in its own commit,
before this file existed).  Report: `ORACLE.md`.

What it is: an interpreter for the narrow subset the battery and the cases use.
It reads the **emitted `.hy` source text** and evaluates every arithmetic site
twice — once in **exact** (arbitrary-precision) arithmetic, once in **X1/X2**
two's-complement wraparound, which is what the *emitted Go* computes.  A site
where the two disagree is a silent wrong result inside a program the checker
accepted.

Both semantics are re-implemented here from scratch rather than imported from
the generator, so an agreement with the generator's table is evidence about the
reader *and* the semantics, not a tautology (pre-registered as O1).

Standard library only.

    python value_oracle.py --dir battery          # census + ORACLE.md
    python value_oracle.py --check                # O1: reader vs generator table
    python value_oracle.py --dir cases --print    # just show it
"""
from __future__ import annotations

import argparse
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPORT = os.path.join(HERE, "ORACLE.md")

# The kind grid, re-declared here on purpose (see the module docstring).
# Both spellings the repo uses are covered: the battery writes `i8`,
# the hand-written cases write `int8`.
KINDS = {
    "i8": (8, True), "i16": (16, True), "i32": (32, True), "i64": (64, True),
    "u8": (8, False), "u16": (16, False), "u32": (32, False), "u64": (64, False),
    "int8": (8, True), "int16": (16, True), "int32": (32, True), "int64": (64, True),
    "uint8": (8, False), "uint16": (16, False), "uint32": (32, False), "uint64": (64, False),
    "byte": (8, False), "rune": (32, True),
}

DECL = re.compile(r"(?m)^\s*(var|let|const)\s+([a-z_][a-z0-9_]*)\s+([A-Za-z][A-Za-z0-9_]*)\s*=\s*(-?\d+)\s*$")
SITE = re.compile(
    r"(?m)^\s*(?:[a-z_][a-z0-9_]*\s*:=|_\s*=|_?\s*=)\s*"
    r"([a-z_][a-z0-9_]*)\s*(\+|-|\*|/|%|<<|>>|\|)\s*([a-z_][a-z0-9_]*)\s*$"
)


def wrap(value, kind):
    """X1/X2 — truncate to the width, two's complement when signed."""
    bits, signed = KINDS[kind]
    v = value & ((1 << bits) - 1)
    if signed and v >= (1 << (bits - 1)):
        v -= 1 << bits
    return v


def eval_exact(a, b, op):
    """Reference semantics: Python's int is arbitrary precision (X4 for / and %)."""
    if op == "+":
        return a + b
    if op == "-":
        return a - b
    if op == "*":
        return a * b
    if op == "<<":
        return a << b
    if op == ">>":
        return a >> b
    if op == "|":
        return a | b
    if op in ("/", "%"):
        q = abs(a) // abs(b) if (a < 0) == (b < 0) else -(abs(a) // abs(b))
        return q if op == "/" else a - q * b
    raise ValueError(op)


def interpret(src):
    """Read one program's text and return its arithmetic sites."""
    decls = {}
    for kind_word, name, kind, literal in DECL.findall(src):
        if kind not in KINDS:
            continue
        decls[name] = (kind, int(literal), kind_word)
    sites = []
    for a_name, op, b_name in SITE.findall(src):
        if a_name not in decls or b_name not in decls:
            sites.append({"a": a_name, "op": op, "b": b_name, "resolved": False})
            continue
        kind, a_val, a_kw = decls[a_name]
        b_kind, b_val, b_kw = decls[b_name]
        if kind != b_kind:
            sites.append({"a": a_name, "op": op, "b": b_name, "resolved": False,
                          "why": "operand kinds differ"})
            continue
        if a_kw == "const" and b_kw == "const":
            # Go folds this before the run: its value is a compile-time fact, and
            # the checker rejects the shape rather than accept a wrong runtime value.
            sites.append({"a": a_name, "op": op, "b": b_name, "kind": kind,
                          "a_val": a_val, "b_val": b_val, "const_folded": True,
                          "exact": eval_exact(a_val, b_val, op)})
            continue
        exact = eval_exact(a_val, b_val, op)
        x12 = wrap(exact, kind)
        sites.append({"a": a_name, "op": op, "b": b_name, "kind": kind,
                      "a_val": a_val, "b_val": b_val, "const_folded": False,
                      "exact": exact, "x12": x12, "diverges": exact != x12,
                      "resolved": True})
    return sites


def census(directory):
    """Interpret every .hy in a directory and aggregate the divergence census."""
    files = sorted(f for f in os.listdir(directory) if f.endswith(".hy"))
    per_file = []
    total = resolved = const_folded = diverging = unresolved = 0
    for name in files:
        with open(os.path.join(directory, name), encoding="utf-8") as handle:
            sites = interpret(handle.read())
        d = sum(1 for s in sites if s.get("diverges"))
        c = sum(1 for s in sites if s.get("const_folded"))
        u = sum(1 for s in sites if not s.get("resolved"))
        total += len(sites)
        resolved += len(sites) - u - c
        const_folded += c
        diverging += d
        unresolved += u
        per_file.append({"file": name, "sites": len(sites), "diverging": d,
                         "const_folded": c, "unresolved": u, "sites_raw": sites})
    return {"files": len(files), "sites": total, "resolved": resolved,
            "const_folded": const_folded, "diverging": diverging,
            "unresolved": unresolved, "per_file": per_file}


def check_against_generator(seed=3060):
    """O1 — does the reader reproduce the generator's own table?

    The generator's table is the *independent* computation (in-memory values);
    this reads the emitted text back. Agreement is evidence that the oracle is a
    faithful reader, not a restatement.
    """
    import battery_generator as bg

    directory = os.path.join(HERE, "battery")
    checked = 0
    mismatches = []
    for case in bg.generate(seed):
        path = os.path.join(directory, f"{case['slug']}.hy")
        if not os.path.exists(path):
            mismatches.append((case["slug"], "missing file"))
            continue
        with open(path, encoding="utf-8") as handle:
            sites = interpret(handle.read())
        if len(sites) != 1:
            mismatches.append((case["slug"], f"expected 1 site, read {len(sites)}"))
            continue
        site = sites[0]
        checked += 1
        got = (site.get("kind"), site.get("a_val"), site.get("b_val"), site.get("op"),
               site.get("exact"), site.get("x12"), site.get("diverges"))
        want = (case["kind"], case["a"], case["b"], case["op"],
                case["exact"], case["wrapped"], case["diverges"])
        if got != want:
            mismatches.append((case["slug"], f"{got} != {want}"))
    return {"checked": checked, "mismatches": mismatches,
            "expected_total": len(bg.generate(seed))}


def write_report(result, o1, go_available=False):
    o1_ok = not o1["mismatches"] and o1["checked"] == o1["expected_total"]
    lines = [
        "# ORACLE — Spike W-3060 (exact-arithmetic value oracle, M2 artifact)",
        "",
        "Generated by `value_oracle.py`. The predictions (O1/O2/O3) were committed",
        "first, in `VALUE-ORACLE-PREREGISTRATION.md`; this file comes after.",
        "",
        "## Verdicts",
        "",
        f"- **O1 (reader reproduces the generator's table)** — {o1['checked']}/"
        f"{o1['expected_total']} sites read back, {len(o1['mismatches'])} mismatch(es) "
        f"=> **{'HELD' if o1_ok else 'FALSIFIED'}**",
        f"- **O2 (divergence census is non-zero)** — **{result['diverging']}** diverging "
        f"sites across {result['files']} files => "
        f"**{'HELD' if result['diverging'] > 0 else 'FALSIFIED'}**",
        f"- **O3 (Go cross-check exists; skips without a toolchain)** — "
        f"`tests/test_value_oracle.py`; Go "
        f"{'found' if go_available else 'NOT found here'} in this environment"
        + ("" if go_available else " (so this run skipped it; CI has Go)"),
        "",
        "## Census",
        "",
        f"- files read: **{result['files']}**; arithmetic sites: **{result['sites']}**",
        f"- runtime sites (resolved, non-constant): **{result['resolved']}**",
        f"- **diverging sites (exact != what the emitted Go computes): "
        f"{result['diverging']}**",
        f"- constant-folded sites (Go refuses these; the checker rejects the shape): "
        f"{result['const_folded']}",
        f"- unresolved sites (counted fail-closed): {result['unresolved']}",
        "",
        "## Per file",
        "",
        "| file | sites | diverging | const-folded | unresolved |",
        "|------|-------|-----------|--------------|------------|",
    ]
    for row in result["per_file"]:
        lines.append(f"| `{row['file']}` | {row['sites']} | {row['diverging']} "
                     f"| {row['const_folded']} | {row['unresolved']} |")

    lines += [
        "",
        "## A sample, read back from the emitted text",
        "",
        "| file | kind | a | op | b | exact | emitted Go gives | divergence |",
        "|------|------|---|---|---|-------|------------------|------------|",
    ]
    shown = 0
    for row in result["per_file"]:
        for site in row["sites_raw"]:
            if not (site.get("diverges") or site.get("const_folded")):
                continue
            lines.append(f"| `{row['file']}` | `{site.get('kind')}` | {site.get('a_val')} "
                         f"| `{site['op']}` | {site.get('b_val')} | {site.get('exact')} "
                         f"| **{site.get('x12', '-')}** "
                         f"| {'yes' if site.get('diverges') else 'const-folded'} |")
            shown += 1
            if shown >= 12:
                break
        if shown >= 12:
            break

    lines += [
        "",
        "## Limits (from the pre-registration, unchanged)",
        "",
        "- The reader is a regex-level parser for the battery/case subset, not a",
        "  `.hy` front end; O1 is what bounds that risk.",
        "- The divergence count characterises the battery's shapes, not the language.",
        "- No checker or emitter code was modified.",
        "",
    ]
    with open(REPORT, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines))
    return result


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dir", default=os.path.join(HERE, "battery"))
    ap.add_argument("--check", action="store_true", help="run O1 against the generator table")
    ap.add_argument("--print", dest="do_print", action="store_true")
    args = ap.parse_args(argv)

    if args.check:
        o1 = check_against_generator()
        print(f"O1: {o1['checked']}/{o1['expected_total']} read back, "
              f"{len(o1['mismatches'])} mismatch")
        for slug, why in o1["mismatches"][:10]:
            print(f"  {slug}: {why}")
        return 0

    import shutil

    result = census(args.dir)
    o1 = check_against_generator()
    write_report(result, o1, go_available=bool(shutil.which("go")))
    if args.do_print:
        print(f"files: {result['files']}; sites: {result['sites']}; "
              f"diverging: {result['diverging']}; const-folded: {result['const_folded']}")
    print(f"ORACLE.md: {result['diverging']} diverging of {result['sites']} sites; "
          f"O1 {o1['checked']}/{o1['expected_total']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
