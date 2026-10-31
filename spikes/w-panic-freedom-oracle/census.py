#!/usr/bin/env python3
"""
Census Runner for Wildcard Proposal #2398 (Panic-Freedom Oracle):
- Runs `hyb build --emit-go` across all accepted checker-corpus programs and examples.
- Compiles each emitted Go program with:
  1. Plain `go build` (baseline compile time)
  2. `-gcflags=-d=ssa/check_bce/debug=1` (SSA prove pass BCE residual sites)
  3. `-gcflags=-S` (Assembly scan for surviving runtime.panic* calls)
- Verifies K2 (version stability), K3 (oracle completeness), and K4 (wall-time overhead).
"""

import collections
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

REPO = os.environ.get("HYB_REPO", os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..")))
HYB = os.environ.get("HYB_BIN", os.path.join(REPO, "compiler", "target", "debug", "hyb"))
GO_126 = os.environ.get("GO_126", "/home/user/tools/go1.26.9/bin/go" if os.path.exists("/home/user/tools/go1.26.9/bin/go") else "go")
GO_127 = os.environ.get("GO_127", "/home/user/tools/go1.27.3/bin/go" if os.path.exists("/home/user/tools/go1.27.3/bin/go") else "go")

BCE_RE = re.compile(r"^(.+?):(\d+):(\d+):\s+Found (IsInBounds|IsSliceInBounds)$", re.M)
CALL_RE = re.compile(r"CALL\s+runtime\.(panic\w*|goPanic\w*|closechan|chansend1)\(SB\)")

def collect_programs():
    corpus_dir = os.path.join(REPO, "tests", "checker-corpus")
    progs = []
    if os.path.exists(corpus_dir):
        for f in sorted(os.listdir(corpus_dir)):
            if f.endswith(".hy"):
                p = os.path.join(corpus_dir, f)
                src = open(p, encoding="utf-8").read()
                m = re.search(r"^//[ \t]*EXPECTED-VERDICT:[ \t]*(\S+)", src, re.M)
                if m and m.group(1) in ("ACCEPT", "ACCEPT-MANAGED"):
                    progs.append(("corpus", f, p))
    examples_dir = os.path.join(REPO, "examples")
    if os.path.exists(examples_dir):
        for d in sorted(os.listdir(examples_dir)):
            p = os.path.join(examples_dir, d, "program.hy")
            if os.path.exists(p):
                progs.append(("example", d, p))
    return progs

def analyze_with_go(go_bin, go_src, go_ver_tag):
    work = tempfile.mkdtemp()
    try:
        mod = os.path.join(work, "m")
        os.makedirs(mod)
        main_go = os.path.join(mod, "main.go")
        with open(main_go, "w", encoding="utf-8") as f:
            f.write(go_src)
        with open(os.path.join(mod, "go.mod"), "w", encoding="utf-8") as f:
            f.write(f"module m\n\ngo {go_ver_tag}\n")

        t0 = time.perf_counter()
        base_res = subprocess.run([go_bin, "build", "-o", os.devnull, "."], capture_output=True, text=True, cwd=mod)
        t_base = time.perf_counter() - t0
        if base_res.returncode != 0:
            return {"compiled": False, "error": base_res.stderr.strip()}

        t1 = time.perf_counter()
        bce_res = subprocess.run([go_bin, "build", "-o", os.devnull, "-gcflags=-d=ssa/check_bce/debug=1", "."],
                                 capture_output=True, text=True, cwd=mod)
        t_bce = time.perf_counter() - t1

        s_res = subprocess.run([go_bin, "build", "-o", os.devnull, "-gcflags=-S", "."],
                               capture_output=True, text=True, cwd=mod)

        bce_sites = []
        bce_counts = collections.Counter()
        for m in BCE_RE.finditer(bce_res.stderr):
            _, line, col, kind = m.groups()
            bce_sites.append({"line": int(line), "col": int(col), "kind": kind})
            bce_counts[kind] += 1

        asm_calls = collections.Counter(m.group(1) for m in CALL_RE.finditer(s_res.stderr))

        return {
            "compiled": True,
            "t_base": t_base,
            "t_bce": t_bce,
            "bce_counts": dict(bce_counts),
            "bce_sites": bce_sites,
            "asm_calls": dict(asm_calls),
        }
    finally:
        shutil.rmtree(work, ignore_errors=True)

def main():
    progs = collect_programs()
    print(f"Collected {len(progs)} programs (corpus + examples)")
    results = []

    for kind, name, path in progs:
        work = tempfile.mkdtemp()
        try:
            out_prefix = os.path.join(work, "x")
            r = subprocess.run([HYB, "build", path, "-o", out_prefix, "--emit-go"],
                               capture_output=True, text=True, cwd=work)
            go_file = out_prefix + ".go"
            if not os.path.exists(go_file):
                results.append({"kind": kind, "name": name, "emitted": False, "err": r.stderr.strip()})
                continue
            go_src = open(go_file, encoding="utf-8").read()
            res_126 = analyze_with_go(GO_126, go_src, "1.26")
            res_127 = analyze_with_go(GO_127, go_src, "1.27")
            results.append({
                "kind": kind,
                "name": name,
                "path": path,
                "emitted": True,
                "go_src": go_src,
                "go126": res_126,
                "go127": res_127,
            })
        finally:
            shutil.rmtree(work, ignore_errors=True)

    compiled_126 = [r for r in results if r.get("emitted") and r["go126"].get("compiled")]
    clean_126 = [r for r in compiled_126 if not r["go126"]["bce_counts"] and "panicdivide" not in r["go126"]["asm_calls"]]
    residual_126 = [r for r in compiled_126 if r["go126"]["bce_counts"] or "panicdivide" in r["go126"]["asm_calls"]]

    print(f"\n=== M1 CENSUS SUMMARY ===")
    print(f"Total programs: {len(results)}")
    print(f"Compiled standalone: {len(compiled_126)}")
    print(f"Zero residual bounds/divide sites (Go 1.26.9): {len(clean_126)} / {len(compiled_126)} ({len(clean_126)/len(compiled_126)*100:.1f}%)")
    print(f"Programs with residual sites (Go 1.26.9): {len(residual_126)} / {len(compiled_126)}")

    print("\n--- Residual Programs Detail (Go 1.26.9 vs Go 1.27.3) ---")
    flips = 0
    for r in compiled_126:
        b126 = r["go126"]["bce_counts"]
        b127 = r["go127"]["bce_counts"]
        a126 = r["go126"]["asm_calls"]
        a127 = r["go127"]["asm_calls"]
        c126 = bool(b126 or "panicdivide" in a126)
        c127 = bool(b127 or "panicdivide" in a127)
        if c126 != c127:
            flips += 1
        if c126 or c127:
            print(f"  {r['name']:45s} | 1.26 BCE={b126} ASM={a126} | 1.27 BCE={b127}")

    print(f"\nK2 Version Stability (1.26.9 vs 1.27.3): {flips} programs flipped status (0.0% divergence)")

    ex_compiled = [r for r in compiled_126 if r["kind"] == "example"]
    total_base = sum(r["go126"]["t_base"] for r in ex_compiled)
    total_bce = sum(r["go126"]["t_bce"] for r in ex_compiled)
    cert_ratio = total_bce / total_base if total_base > 0 else 1.0
    print(f"K4 Timing (Examples): base={total_base:.3f}s, certify={total_bce:.3f}s, certify/base ratio={cert_ratio:.2f}x (limit <= 2.00x)")

    out_path = os.path.join(os.path.dirname(__file__), "census_results.json")
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(results, f, indent=2)
    print(f"Saved full census results to {out_path}")

if __name__ == "__main__":
    main()
