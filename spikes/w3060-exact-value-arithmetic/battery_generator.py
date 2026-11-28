"""Spike W-3060 — the narrow-width battery generator (M1 artifact 1, #3060).

Pre-registration: `BATTERY-PREREGISTRATION.md` (committed in its own commit,
**before this file existed**).  Results: `BATTERY.md`, written by this script.

It builds the *measuring instrument* the amended M1 item 2 needs: a
deterministic battery of typed, narrow-width arithmetic sites, each emitted as
a whole little `.hy` program with the operands as **variables**, together with
the ground truth (X1/X2 wrapped value vs exact value) and the verdict of the
locked **trivial interval pass**.

Standard library only.

    python battery_generator.py                     # write battery/ + BATTERY.md
    python battery_generator.py --hyb <path>        # ...and record acceptance
    python battery_generator.py --print             # only print the metrics
"""
from __future__ import annotations

import argparse
import os
import random
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
BATTERY = os.path.join(HERE, "battery")
REPORT = os.path.join(HERE, "BATTERY.md")

# (bits, signed) — the locked kind grid.
KINDS = {
    "i8": (8, True), "i16": (16, True), "i32": (32, True), "i64": (64, True),
    "u8": (8, False), "u16": (16, False), "u32": (32, False), "u64": (64, False),
}
OPS = ["+", "-", "*", "/", "%", "<<", ">>", "|"]
DRAWS = ["max-saturated", "near-max", "uniform"]


def range_of(kind):
    bits, signed = KINDS[kind]
    if signed:
        return -(1 << (bits - 1)), (1 << (bits - 1)) - 1
    return 0, (1 << bits) - 1


def wrap(value, kind):
    """SPEC §11 X1/X2: truncate to the kind's width, two's complement if signed."""
    bits, signed = KINDS[kind]
    v = value & ((1 << bits) - 1)
    if signed and v >= (1 << (bits - 1)):
        v -= 1 << bits
    return v


def exact_op(a, b, op):
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
    if op == "/":
        return abs(a) // abs(b) if (a < 0) == (b < 0) else -(abs(a) // abs(b))
    if op == "%":
        q = abs(a) // abs(b) if (a < 0) == (b < 0) else -(abs(a) // abs(b))
        return a - q * b
    raise ValueError(op)


def draw_operands(kind, op, draw, rng):
    """The three locked draws (boundary-weighted on purpose)."""
    lo, hi = range_of(kind)
    if draw == "max-saturated":
        a, b = hi, hi
    elif draw == "near-max":
        a, b = hi, min(2, hi)
    else:
        a = rng.randint(lo, hi)
        b = rng.randint(lo, hi)
    if op in ("/", "%"):
        if b == 0:                                   # divisor guard (locked)
            b = 1
        if draw == "uniform" and KINDS[kind][1]:     # the min / -1 shape (locked)
            a, b = lo, -1
    if op in ("<<", ">>"):
        b = abs(b) % KINDS[kind][0]                  # a shift count inside the width
    return a, b


SLUG = {"+": "add", "-": "sub", "*": "mul", "/": "div", "%": "mod",
        "<<": "shl", ">>": "shr", "|": "or"}


def build_case(kind, op, draw, rng):
    a, b = draw_operands(kind, op, draw, rng)
    wrapped = wrap(exact_op(a, b, op), kind)
    exact = exact_op(a, b, op)
    lo, hi = range_of(kind)
    # The locked trivial interval pass: interval(literal) is a point, so the
    # result interval is the exact result; `exact` iff it fits the kind.
    provable_exact = lo <= exact <= hi
    return {
        "id": f"{kind}-{op}-{draw}",
        "slug": f"{kind}-{SLUG[op]}-{draw.replace('-', '')}",
        "kind": kind, "op": op, "draw": draw, "a": a, "b": b,
        "exact": exact, "wrapped": wrapped, "diverges": wrapped != exact,
        "provable_exact": provable_exact,
        "src": (
            f"// W-3060 battery: `{a} {op} {b}` on typed {kind} variables ({draw} draw).\n"
            "fn main() {\n"
            f"    var a {kind} = {a}\n"
            f"    var b {kind} = {b}\n"
            f"    c := a {op} b\n"
            "    fmt.println(c)\n"
            "}\n"
        ),
    }


def generate(seed=3060):
    rng = random.Random(seed)
    cases = []
    for kind in KINDS:
        for op in OPS:
            for draw in DRAWS:
                cases.append(build_case(kind, op, draw, rng))
    return cases


def metrics(cases):
    n = len(cases)
    diverging = [c for c in cases if c["diverges"]]
    discharged = [c for c in cases if c["provable_exact"]]
    per_op = {}
    for c in cases:
        slot = per_op.setdefault(c["op"], {"n": 0, "div": 0, "exact": 0})
        slot["n"] += 1
        slot["div"] += 1 if c["diverges"] else 0
        slot["exact"] += 1 if c["provable_exact"] else 0
    return {
        "n": n,
        "diverging": len(diverging),
        "diverging_share": len(diverging) / n if n else 0.0,
        "discharged": len(discharged),
        "discharge_rate": len(discharged) / n if n else 0.0,
        "per_op": per_op,
    }


def emit(cases, directory=BATTERY):
    os.makedirs(directory, exist_ok=True)
    written = []
    for case in cases:
        path = os.path.join(directory, f"{case['slug']}.hy")
        with open(path, "w", encoding="utf-8", newline="\n") as handle:
            handle.write(case["src"])
        written.append(f"{case['slug']}.hy")
    return written


def accepts(hyb, path):
    proc = subprocess.run([hyb, "check", path], capture_output=True, text=True,
                          encoding="utf-8", errors="replace")
    text = (proc.stdout or "") + (proc.stderr or "")
    return "OK (0 diagnostics)" in text or proc.returncode == 0


def write_report(cases, stat, hyb=None, seed=3060):
    lines = [
        "# BATTERY — Spike W-3060 (narrow-width generator, M1 artifact 1)",
        "",
        f"Generated by `battery_generator.py --seed {seed}`. The sampling design,",
        "the analysis rule and the three predicted outcomes were committed first, in",
        "`BATTERY-PREREGISTRATION.md`; this file comes after.",
        "",
        "## Verdicts",
        "",
        f"- **P1 (non-vacuous battery: >= 1/3 diverging)** — {stat['diverging']}/{stat['n']} "
        f"= **{stat['diverging_share']:.1%}** => "
        f"**{'HELD' if stat['diverging_share'] >= 1 / 3 else 'FALSIFIED'}**",
        f"- **P2 (interval pass discharges >= 70%)** — {stat['discharged']}/{stat['n']} "
        f"= **{stat['discharge_rate']:.1%}** => "
        f"**{'HELD' if stat['discharge_rate'] >= 0.70 else 'NO'}**"
        f" (the #3060 defeat condition fires only below 50%)",
    ]
    if hyb is None:
        lines.append("- **P3 (every program accepted)** — not measured in this run "
                     "(no `--hyb`); reproduce with `--hyb <path-to-hyb>`")
    else:
        accepted = 0
        rejected = []
        for case in cases:
            path = os.path.join(BATTERY, f"{case['slug']}.hy")
            if accepts(hyb, path):
                accepted += 1
            else:
                rejected.append(case["slug"])
        lines.append(f"- **P3 (every program accepted)** — {accepted}/{stat['n']} => "
                     f"**{'HELD' if accepted == stat['n'] else 'FALSIFIED'}**"
                     + (f"; rejected: {', '.join(rejected[:6])}" if rejected else ""))

    lines += [
        "",
        "## Discharge rate by operator",
        "",
        "| op | sites | diverging | provably exact | discharge rate |",
        "|----|-------|-----------|----------------|----------------|",
    ]
    for op in OPS:
        slot = stat["per_op"][op]
        rate = slot["exact"] / slot["n"] if slot["n"] else 0.0
        lines.append(f"| `{op}` | {slot['n']} | {slot['div']} | {slot['exact']} | {rate:.0%} |")

    lines += [
        "",
        "## The battery",
        "",
        "| id | kind | op | draw | a | b | exact | wrapped (X1/X2) | diverges | provably exact |",
        "|----|------|----|------|---|---|-------|-----------------|----------|----------------|",
    ]
    for case in cases:
        lines.append(
            f"| `{case['id']}` | `{case['kind']}` | `{case['op']}` | {case['draw']} "
            f"| {case['a']} | {case['b']} | {case['exact']} | **{case['wrapped']}** "
            f"| {'yes' if case['diverges'] else 'no'} "
            f"| {'yes' if case['provable_exact'] else 'no'} |"
        )

    lines += [
        "",
        "## Limits (from the pre-registration, unchanged)",
        "",
        "- No checker instrumentation; `hyb check` is used only to confirm the sites",
        "  are inside the checked subset (P3).",
        "- Integers only — floats are the stated non-goal of #3060.",
        "- The pass is deliberately trivial (literal intervals only), so the",
        "  discharge rate is a **floor**: a stronger pass would raise it.",
        "",
    ]
    with open(REPORT, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(lines))
    return stat


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--seed", type=int, default=3060)
    ap.add_argument("--hyb", default=None, help="path to a built `hyb` binary")
    ap.add_argument("--print", dest="do_print", action="store_true")
    args = ap.parse_args(argv)

    cases = generate(args.seed)
    stat = metrics(cases)
    if args.do_print:
        print(f"sites: {stat['n']}; diverging: {stat['diverging']} "
              f"({stat['diverging_share']:.1%}); discharged: {stat['discharged']} "
              f"({stat['discharge_rate']:.1%})")
    written = emit(cases)
    if args.do_print:
        print(f"emitted {len(written)} programs into battery/")
    write_report(cases, stat, args.hyb, args.seed)
    print(f"BATTERY.md: {stat['n']} sites, {stat['diverging']} diverging, "
          f"{stat['discharged']} provably exact")
    return 0


if __name__ == "__main__":
    sys.exit(main())

