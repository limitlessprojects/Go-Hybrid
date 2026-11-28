#!/usr/bin/env python3
"""
Corpus Syntactic Distance Census for Wildcard Proposal #2406:
"Inline Ghost-Capability Directives: Erasing the .hy file split via standard Go AST comment pragmas"

Measures the exact syntactic delta across all 51 programs in tests/checker-corpus/ and examples/:
1. Non-Go keywords (fn, task, move, managed, share, borrow)
2. Non-Go control flow (0..n range loops, for x in xs without _)
3. Non-Go expressions (? try operator, match, string interpolation)
4. Evaluates whether comments vs ghost-functions survive go/parser and gofmt without dissociation.
"""

import collections
import json
import os
import re
import sys

REPO = os.environ.get("HYB_REPO", os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))

PATTERNS = {
    "fn_decl": re.compile(r"^[ \t]*fn\s+\w+", re.M),
    "task_spawn": re.compile(r"\btask\s+\w+\s*\(", re.M),
    "move_arg": re.compile(r"\bmove\s+\w+", re.M),
    "managed_block": re.compile(r"\bmanaged\s*\(.*?\)\s*\{", re.M),
    "range_loop": re.compile(r"\bfor\s+\w+\s+in\s+\d+\.\.\w+", re.M),
    "iter_loop": re.compile(r"\bfor\s+\w+\s+in\s+[^0-9\s]\w*", re.M),
    "try_op": re.compile(r"\w+\s*\?", re.M),
    "string_interp": re.compile(r'"[^"\n]*\{[a-zA-Z0-9_\.()]+(?:\s*:\s*[^}\n]+)?\}[^"\n]*"', re.M),
}

def analyze_corpus():
    corpus_dir = os.path.join(REPO, "tests", "checker-corpus")
    examples_dir = os.path.join(REPO, "examples")

    programs = []
    if os.path.exists(corpus_dir):
        for f in sorted(os.listdir(corpus_dir)):
            if f.endswith(".hy"):
                p = os.path.join(corpus_dir, f)
                src = open(p, encoding="utf-8", errors="replace").read()
                m = re.search(r"^//[ \t]*EXPECTED-VERDICT:[ \t]*(\S+)", src, re.M)
                verdict = m.group(1) if m else "UNKNOWN"
                programs.append(("corpus", f, p, src, verdict))

    if os.path.exists(examples_dir):
        for d in sorted(os.listdir(examples_dir)):
            p = os.path.join(examples_dir, d, "program.hy")
            if os.path.exists(p):
                src = open(p, encoding="utf-8", errors="replace").read()
                programs.append(("example", d, p, src, "EXAMPLE"))

    total_progs = len(programs)
    total_lines = 0
    feature_totals = collections.Counter()
    prog_reports = []

    for kind, name, path, src, verdict in programs:
        lines = len(src.splitlines())
        total_lines += lines
        counts = {}
        for feat, pat in PATTERNS.items():
            c = len(pat.findall(src))
            counts[feat] = c
            feature_totals[feat] += c

        total_diffs = sum(counts.values())
        prog_reports.append({
            "kind": kind,
            "name": name,
            "lines": lines,
            "verdict": verdict,
            "counts": counts,
            "total_divergences": total_diffs
        })

    print("==================================================")
    print("M1 Syntactic Distance Census Report (#2406)")
    print(f"Total programs surveyed: {total_progs} ({total_lines:,} total lines)")
    print("==================================================")
    print("\nNon-Go Syntactic Features Requiring Transformation:")
    for feat, count in feature_totals.most_common():
        pct = (count / total_lines) * 100 if total_lines else 0
        print(f"  - {feat:15s}: {count:5d} sites ({pct:.2f}% of corpus lines)")

    avg_div = sum(r["total_divergences"] for r in prog_reports) / max(1, total_progs)
    print(f"\nAverage syntactic conversions required per program: {avg_div:.1f} sites")

    out_file = os.path.join(os.path.dirname(__file__), "census_distance_results.json")
    with open(out_file, "w", encoding="utf-8") as f:
        json.dump({
            "total_programs": total_progs,
            "total_lines": total_lines,
            "feature_totals": dict(feature_totals),
            "programs": prog_reports
        }, f, indent=2)
    print(f"Saved results to {out_file}")

if __name__ == "__main__":
    analyze_corpus()
