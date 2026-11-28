"""Spike W-3060 — exact-value arithmetic: the silence-window census.

Pre-registration: `PREREGISTRATION.md` (committed in its own commit, before
this script was ever run).  Results: `RESULTS.md`, written by this script.

Models SPEC §11 **X1/X2** (two's-complement wraparound with an explicit width)
against **exact** arithmetic (Python's `int` is arbitrary precision — that *is*
the reference semantics), and emits one `.hy` file per spelling into `cases/`
so every checker verdict is reproducible.

Standard library only.

    python exact_value_arithmetic.py                 # write cases/ + RESULTS.md
    python exact_value_arithmetic.py --hyb <path>    # ...and record verdicts
    python exact_value_arithmetic.py --print         # only print the table
"""
from __future__ import annotations

import argparse
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
CASES = os.path.join(HERE, "cases")
RESULTS = os.path.join(HERE, "RESULTS.md")

# --------------------------------------------------------------------------
# SPEC §11 X1/X2 — two's-complement wrapping with an explicit width.
# --------------------------------------------------------------------------
KINDS = {
    "int8": (8, True), "int16": (16, True), "int32": (32, True), "int64": (64, True),
    "uint8": (8, False), "uint16": (16, False), "uint32": (32, False), "uint64": (64, False),
    "int": (64, True), "uint": (64, False),
}


def wrap(value: int, kind: str) -> int:
    """X1/X2: truncate to the kind's width, two's complement when signed."""
    bits, signed = KINDS[kind]
    v = value & ((1 << bits) - 1)
    if signed and v >= (1 << (bits - 1)):
        v -= 1 << bits
    return v


def exact(a: int, b: int, op: str) -> int:
    return {"+": a + b, "-": a - b, "*": a * b, "<<": a << b}[op]


def wrapped(a: int, b: int, op: str, kind: str) -> int:
    """What the emitted Go computes for typed operands (X1/X2)."""
    return wrap(exact(a, b, op), kind)


# --------------------------------------------------------------------------
# The pre-registered shapes — prediction frozen in PREREGISTRATION.md.
# (id, kind, a, b, op, predicted_wrapped, predicted_exact)
# --------------------------------------------------------------------------
SHAPES = [
    ("s1",  "int8",   100, 100, "+",  -56, 200),
    ("s2",  "int16",  32767, 1, "+",  -32768, 32768),
    ("s3",  "int32",  2000000000, 2000000000, "+", -294967296, 4000000000),
    ("s4",  "int64",  9223372036854775807, 1, "+", -9223372036854775808, 9223372036854775808),
    ("s5",  "uint8",  200, 100, "+",  44, 300),
    ("s6",  "uint32", 4000000000, 1000000000, "+", 705032704, 5000000000),
    ("s7",  "uint64", 18446744073709551615, 1, "+", 0, 18446744073709551616),
    ("s8",  "int8",   100, 2, "*",  -56, 200),
    ("s9",  "int32",  1, 31, "<<", -2147483648, 2147483648),
    ("s10", "int8",   -128, 1, "-",  127, -129),
]

H1_THRESHOLD = 8  # PREREGISTRATION.md: ">= 8 of 10 diverge"


def rows():
    out = []
    for sid, kind, a, b, op, pred_w, pred_e in SHAPES:
        w, e = wrapped(a, b, op, kind), exact(a, b, op)
        out.append({
            "id": sid, "kind": kind, "a": a, "b": b, "op": op,
            "wrapped": w, "exact": e,
            "diverges": w != e,
            "prediction_held": (w == pred_w and e == pred_e and w != e),
        })
    return out


# --------------------------------------------------------------------------
# Emission — one .hy per spelling, so a reviewer can reproduce every verdict.
# --------------------------------------------------------------------------
def _vars_src(sid, kind, a, b, op):
    return (
        f"// W-3060 shape {sid}: `{a} {op} {b}` on typed {kind} VARIABLES.\n"
        "// Pre-registered: accepted by `hyb check` (0 diagnostics); wraps at run time (X1/X2).\n"
        "fn main() {\n"
        f"    var a {kind} = {a}\n"
        f"    var b {kind} = {b}\n"
        f"    c := a {op} b\n"
        "    fmt.println(c)\n"
        "}\n"
    )


def _consts_src(sid, kind, a, b, op):
    return (
        f"// W-3060 shape {sid}: the SAME numbers as typed {kind} CONSTANTS.\n"
        "// Pre-registered: rejected by `hyb check` (Go folds the constant and refuses).\n"
        f"const a {kind} = {a}\n"
        f"const b {kind} = {b}\n"
        "fn main() {\n"
        f"    _ = a {op} b\n"
        "}\n"
    )


def emit_cases(directory=CASES):
    os.makedirs(directory, exist_ok=True)
    written = []
    for sid, kind, a, b, op, _w, _e in SHAPES:
        for suffix, src in (("vars", _vars_src(sid, kind, a, b, op)),
                            ("consts", _consts_src(sid, kind, a, b, op))):
            path = os.path.join(directory, f"{sid}-{suffix}.hy")
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(src)
            written.append(f"{sid}-{suffix}.hy")
    return written


# --------------------------------------------------------------------------
# Verdicts — optional, only when a `hyb` binary is supplied.
# --------------------------------------------------------------------------
def parse_verdict(text, returncode=0):
    """('accept'|'reject', code) — the first error code, or OK."""
    if "OK (0 diagnostics)" in text or returncode == 0:
        return "accept", "OK"
    for line in text.splitlines():
        if line.startswith("error[HYB-"):
            return "reject", line.split("]", 1)[0][len("error["):]
    return "reject", "?"


def verdict(hyb, path):
    """Run `<hyb> check <path>` and read the verdict out of its output."""
    proc = subprocess.run([hyb, "check", path], capture_output=True, text=True,
                          encoding="utf-8", errors="replace")
    return parse_verdict((proc.stdout or "") + (proc.stderr or ""), proc.returncode)


def write_results(hyb=None):
    table = rows()
    div = [r for r in table if r["diverges"]]
    held = [r for r in table if r["prediction_held"]]
    lines = [
        "# RESULTS — Spike W-3060 (exact-value arithmetic: silence-window census)",
        "",
        "Generated by `exact_value_arithmetic.py`. The hypothesis and the predicted",
        "table were committed first (`PREREGISTRATION.md`), then this file.",
        "",
        "## H1 — the window is not empty",
        "",
        f"- shapes: **{len(table)}**; diverging (wrapped vs exact): **{len(div)}**; "
        f"predictions held: **{len(held)}/{len(table)}**",
        f"- pre-registered threshold: **>= {H1_THRESHOLD}** diverging => "
        f"**{'HELD' if len(div) >= H1_THRESHOLD else 'FALSIFIED'}**",
        "",
        "| id | kind | expression | wrapped (X1/X2) | exact | diverges | prediction held |",
        "|----|------|-----------|-----------------|-------|----------|-----------------|",
    ]
    for r in table:
        lines.append(
            f"| {r['id']} | `{r['kind']}` | `{r['a']} {r['op']} {r['b']}` | "
            f"**{r['wrapped']}** | {r['exact']} | {'yes' if r['diverges'] else 'no'} | "
            f"{'yes' if r['prediction_held'] else 'NO'} |"
        )

    lines += ["", "## H2 — the verdicts hinge on spelling", ""]
    if hyb is None:
        lines += [
            "Not measured in this run (no `--hyb` given). Reproduce with:",
            "",
            "```sh",
            "python exact_value_arithmetic.py --hyb <path-to-hyb>",
            "```",
        ]
    else:
        lines += ["| case | expected | measured |", "|---|---|---|"]
        for name in sorted(os.listdir(CASES)):
            path = os.path.join(CASES, name)
            v, code = verdict(hyb, path)
            want = "accept" if name.endswith("-vars.hy") else "reject"
            lines.append(f"| `cases/{name}` | {want} | **{v} ({code})** |")

    lines += [
        "",
        "## Limits (stated up front in the pre-registration)",
        "",
        "- No corpus-wide static-discharge rate: that needs a real AST plus an",
        "  interval pass and is pre-registered as M1 item 2 **in the wildcard",
        "  issue**, not here.",
        "- `wrapping` semantics stay exactly today's X1; nothing here proposes a",
        "  run-time trap.",
        "",
    ]
    with open(RESULTS, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines))
    return len(div), len(held), len(table)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--hyb", default=None, help="path to a built `hyb` binary")
    ap.add_argument("--print", dest="do_print", action="store_true")
    ap.add_argument("--cases-only", action="store_true")
    args = ap.parse_args(argv)

    table = rows()
    if args.do_print:
        for r in table:
            state = "diverges" if r["diverges"] else "agrees"
            print(f"{r['id']:>4} {r['kind']:>7} {r['a']} {r['op']} {r['b']} "
                  f"-> wrapped {r['wrapped']}, exact {r['exact']} ({state})")
    written = emit_cases()
    if args.cases_only:
        print(f"cases: {len(written)}")
        return 0
    nd, nh, nt = write_results(args.hyb)
    print(f"RESULTS.md: {nt} shapes, {nd} diverging, {nh} predictions held")
    return 0


if __name__ == "__main__":
    sys.exit(main())

