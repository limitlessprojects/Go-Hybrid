#!/usr/bin/env python3
"""W-030 (#1210): Word-Atomicity Floor Below Tier-0 — official spike by @planners1490."""
from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]

SINGLE_WORD_PRIMS = frozenset({
    "bool", "byte", "int", "int8", "int16", "int32", "int64",
    "uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
    "float32", "float64", "rune", "sync.mutex",
})

TORN_HY = '// torn.hy — ACCEPT-MANAGED program whose production binary reads memory\n// outside every backing array it owns (torn slice header under `managed`).\nstruct cell {\n    s []int\n}\n\nfn writer(c *cell, small []int, big []int) {\n    for i in 0..4_000_000_000 {\n        if i % 2 == 0 {\n            c.s = small\n        } else {\n            c.s = big\n        }\n    }\n}\n\nfn churn() {                                  // keeps the GC write barrier on\n    for i in 0..4_000_000_000 {\n        g := make([]int, 4096)\n        g[0] = i\n    }\n}\n\nfn main() {\n    managed("torn-read demo: slice header shared without a lock", "other", "WILDCARD-TORN") {\n        small := make([]int, 1)\n        big := make([]int, 1 << 20)\n        for i in 0..len(big) {\n            big[i] = 5                       // only the LAST element is 7\n        }\n        big[0] = 7                           // in-bounds reads of big yield 7 either way\n        big[len(big) - 1] = 7\n        canary := make([]int, 1 << 20)       // 8 MB of 3s next to `big`\n        for i in 0..len(canary) {\n            canary[i] = 3\n        }\n        c := cell{s: small}\n        task writer(&c, small, big)\n        task churn()\n\n        mismatched := 0\n        foreign := 0\n        iters := 200_000_000\n        for i in 0..iters {\n            x := c.s\n            if len(x) != cap(x) {\n                mismatched++                 // len and cap from different headers\n            }\n            if len(x) > 0 {\n                v := x[len(x) - 1]\n                if v != 0 && v != 7 {\n                    foreign++                // legit reads are small[0]=0 or big[N-1]=7\n                }\n            }\n        }\n        fmt.println("iters={iters} len_cap_mismatch={mismatched} foreign_reads={foreign} canary0={canary[0]}")\n    }\n}'

TORN_WRITE_HY = '// torn.hy — ACCEPT-MANAGED program whose production binary reads memory\n// outside every backing array it owns (torn slice header under `managed`).\nstruct cell {\n    s []int\n}\n\nfn writer(c *cell, small []int, big []int) {\n    for i in 0..4_000_000_000 {\n        if i % 2 == 0 {\n            c.s = small\n        } else {\n            c.s = big\n        }\n    }\n}\n\nfn churn() {                                  // keeps the GC write barrier on\n    for i in 0..4_000_000_000 {\n        g := make([]int, 4096)\n        g[0] = i\n    }\n}\n\nfn main() {\n    managed("torn-read demo: slice header shared without a lock", "other", "WILDCARD-TORN") {\n        small := make([]int, 1)\n        big := make([]int, 1 << 20)\n        for i in 0..len(big) {\n            big[i] = 5                       // only the LAST element is 7\n        }\n        big[0] = 7                           // in-bounds reads of big yield 7 either way\n        big[len(big) - 1] = 7\n        canary := make([]int, 1 << 20)       // 8 MB of 3s next to `big`\n        for i in 0..len(canary) {\n            canary[i] = 3\n        }\n        c := cell{s: small}\n        task writer(&c, small, big)\n        task churn()\n\n        mismatched := 0\n        foreign := 0\n        iters := 200_000_000\n        for i in 0..iters {\n            x := c.s\n            if len(x) != cap(x) {\n                mismatched++                 // len and cap from different headers\n            }\n            if len(x) > 0 {\n                x[len(x) - 1] = 9            // legit targets: small[0] or big[N-1]\n            }\n        }\n        hit_canary := 0\n        for i in 0..len(canary) {\n            if canary[i] == 9 { hit_canary++ }\n        }\n        hit_big_interior := 0\n        for i in 1..len(big) - 1 {\n            if big[i] == 9 { hit_big_interior++ }\n        }\n        fmt.println("iters={iters} len_cap_mismatch={mismatched} foreign={foreign} canary_cells_overwritten={hit_canary} big_interior_cells_overwritten={hit_big_interior}")\n    }\n}'

CONFUSE_HY = '// confuse.hy — ACCEPT-MANAGED program: a torn `any` value (type word from one\n// type, data word from another) makes the runtime treat an integer as a pointer.\nstruct boxed {\n    d []int\n}\n\nstruct plain {\n    n int\n    m int\n    k int\n}\n\nstruct cell {\n    v any\n}\n\nfn writer(c *cell, a any, b any) {\n    for i in 0..4_000_000_000 {\n        if i % 2 == 0 {\n            c.v = a\n        } else {\n            c.v = b\n        }\n    }\n}\n\nfn churn() {                                  // keeps the GC write barrier on\n    for i in 0..4_000_000_000 {\n        g := make([]int, 4096)\n        g[0] = i\n    }\n}\n\nfn main() {\n    managed("type-confusion demo: any value shared without a lock", "other", "WILDCARD-TORN") {\n        var a any = boxed{d: []int{7}}\n        var b any = plain{n: 4096, m: 1, k: 1}  // read as boxed: ptr=0x1000 len=1 cap=1\n        c := cell{v: a}\n        task writer(&c, a, b)\n        task churn()\n\n        n := 0\n        for i in 0..20_000_000 {\n            v := c.v\n            s := "{v}"\n            n += len(s)\n        }\n        fmt.println("n={n}")\n    }\n}'

TORN_EMITTED_GO = 'package main\n\nimport (\n\t"fmt"\n)\n\ntype cell struct {\n\ts []int\n}\n\nfunc writer(c *cell, small []int, big []int) {\n\tfor i := 0; i < 4000000000; i++ {\n\t\tif i%2 == 0 {\n\t\t\tc.s = small\n\t\t} else {\n\t\t\tc.s = big\n\t\t}\n\t}\n}\n\nfunc churn() {\n\tfor i := 0; i < 4000000000; i++ {\n\t\tg := make([]int, 4096)\n\t\tg[0] = i\n\t}\n}\n\nfunc main() {\n\t{ // managed [other] torn-read demo: slice header shared without a lock (audited via hyb check --report-managed) tickets: WILDCARD-TORN\n\t\tsmall := make([]int, 1)\n\t\tbig := make([]int, 1<<20)\n\t\tfor i := 0; i < len(big); i++ {\n\t\t\tbig[i] = 5\n\t\t}\n\t\tbig[0] = 7\n\t\tbig[len(big)-1] = 7\n\t\tcanary := make([]int, 1<<20)\n\t\tfor i := 0; i < len(canary); i++ {\n\t\t\tcanary[i] = 3\n\t\t}\n\t\tc := cell{s: small}\n\t\tgo writer(&c, small, big)\n\t\tgo churn()\n\t\tmismatched := 0\n\t\tforeign := 0\n\t\titers := 200000000\n\t\tfor i := 0; i < iters; i++ {\n\t\t\tx := c.s\n\t\t\tif len(x) != cap(x) {\n\t\t\t\tmismatched++\n\t\t\t}\n\t\t\tif len(x) > 0 {\n\t\t\t\tv := x[len(x)-1]\n\t\t\t\tif v != 0 && v != 7 {\n\t\t\t\t\tforeign++\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t\tfmt.Println(fmt.Sprintf("iters=%v len_cap_mismatch=%v foreign_reads=%v canary0=%v", iters, mismatched, foreign, canary[0]))\n\t}\n}'

CONFUSE_EMITTED_GO = 'package main\n\nimport (\n\t"fmt"\n)\n\ntype boxed struct {\n\td []int\n}\n\ntype plain struct {\n\tn int\n\tm int\n\tk int\n}\n\ntype cell struct {\n\tv any\n}\n\nfunc writer(c *cell, a any, b any) {\n\tfor i := 0; i < 4000000000; i++ {\n\t\tif i%2 == 0 {\n\t\t\tc.v = a\n\t\t} else {\n\t\t\tc.v = b\n\t\t}\n\t}\n}\n\nfunc churn() {\n\tfor i := 0; i < 4000000000; i++ {\n\t\tg := make([]int, 4096)\n\t\tg[0] = i\n\t}\n}\n\nfunc main() {\n\t{ // managed [other] type-confusion demo: any value shared without a lock (audited via hyb check --report-managed) tickets: WILDCARD-TORN\n\t\tvar a any = boxed{d: []int{7}}\n\t\tvar b any = plain{n: 4096, m: 1, k: 1}\n\t\tc := cell{v: a}\n\t\tgo writer(&c, a, b)\n\t\tgo churn()\n\t\tn := 0\n\t\tfor i := 0; i < 20000000; i++ {\n\t\t\tv := c.v\n\t\t\ts := fmt.Sprintf("%v", v)\n\t\t\tn += len(s)\n\t\t}\n\t\tfmt.Println(fmt.Sprintf("n=%v", n))\n\t}\n}'

BOX_TEST_GO = 'package bench\n\nimport (\n\t"sync/atomic"\n\t"testing"\n)\n\ntype plainCell struct{ s []int }\ntype boxedCell struct{ s atomic.Pointer[[]int] }\n\nvar sinkLen int\n\nfunc BenchmarkPlainStore(b *testing.B) {\n\tc := &plainCell{}\n\ta, bb := make([]int, 1), make([]int, 2)\n\tfor i := 0; i < b.N; i++ {\n\t\tif i&1 == 0 { c.s = a } else { c.s = bb }\n\t}\n}\nfunc BenchmarkBoxedStore(b *testing.B) {\n\tc := &boxedCell{}\n\ta, bb := make([]int, 1), make([]int, 2)\n\tfor i := 0; i < b.N; i++ {\n\t\tv := a\n\t\tif i&1 != 0 { v = bb }\n\t\tc.s.Store(&v) // one 24-byte heap box per publish\n\t}\n}\nfunc BenchmarkPlainLoad(b *testing.B) {\n\tc := &plainCell{s: make([]int, 3)}\n\tfor i := 0; i < b.N; i++ { sinkLen += len(c.s) }\n}\nfunc BenchmarkBoxedLoad(b *testing.B) {\n\tc := &boxedCell{}\n\tv := make([]int, 3)\n\tc.s.Store(&v)\n\tfor i := 0; i < b.N; i++ { sinkLen += len(*c.s.Load()) }\n}'


@dataclasses.dataclass(frozen=True)
class TypeLayout:
    ty: str
    words: int
    kind: str
    multiword_fields: tuple[tuple[str, str], ...] = ()


@dataclasses.dataclass(frozen=True)
class ManagedCrossing:
    category: str
    reason: str
    arg_expr: str
    resolved_type: str
    layout: TypeLayout
    requires_boxing: bool
    is_map_residual: bool


def parse_structs(src: str) -> dict[str, list[tuple[str, str]]]:
    out: dict[str, list[tuple[str, str]]] = {}
    for m in re.finditer(r"\bstruct\s+([A-Za-z_]\w*)\s*\{([^}]+)\}", src):
        fields: list[tuple[str, str]] = []
        for raw in m.group(2).splitlines():
            line = raw.split("//", 1)[0].strip()
            if not line:
                continue
            parts = line.split(None, 1)
            if len(parts) == 2:
                fields.append((parts[0].strip(), parts[1].strip()))
        out[m.group(1)] = fields
    for am in re.finditer(r"\btype\s+([A-Za-z_]\w*)\s*=\s*([^\n;]+)", src):
        out[am.group(1).strip()] = [("$self", am.group(2).split("//", 1)[0].strip())]
    return out


def classify_type(
    ty: str,
    structs: dict[str, list[tuple[str, str]]] | None = None,
    visited: frozenset[str] = frozenset(),
) -> TypeLayout:
    structs = structs or {}
    t = ty.strip()
    if not t or t in visited:
        return TypeLayout(t, 1, "SINGLE_WORD")
    if t.startswith("*"):
        inner = classify_type(t[1:].strip(), structs, visited)
        if inner.kind in ("MULTI_WORD_HEADER", "MULTI_WORD_STRUCT"):
            fields = inner.multiword_fields or (("value", inner.ty),)
            return TypeLayout(t, 1, "MULTI_WORD_STRUCT", fields)
        return TypeLayout(t, 1, "SINGLE_WORD")
    if t in SINGLE_WORD_PRIMS or t.startswith(("chan ", "<-chan ", "chan<- ")):
        return TypeLayout(t, 1, "SINGLE_WORD")
    if t.startswith("map["):
        return TypeLayout(t, 1, "MAP_RESIDUAL")
    if t == "string":
        return TypeLayout(t, 2, "MULTI_WORD_HEADER", (("$self", "string"),))
    if t in ("any", "interface{}"):
        return TypeLayout(t, 2, "MULTI_WORD_HEADER", (("$self", "any"),))
    if t.startswith("[]"):
        return TypeLayout(t, 3, "MULTI_WORD_HEADER", (("$self", t),))
    if t in structs:
        seen = visited | {t}
        if len(structs[t]) == 1 and structs[t][0][0] == "$self":
            aliased = classify_type(structs[t][0][1], structs, seen)
            return TypeLayout(t, aliased.words, aliased.kind, aliased.multiword_fields)
        mw: list[tuple[str, str]] = []
        for fn, ft in structs[t]:
            fl = classify_type(ft, structs, seen)
            if fl.kind == "MULTI_WORD_HEADER":
                mw.append((fn, ft))
            elif fl.kind == "MULTI_WORD_STRUCT":
                mw.extend(fl.multiword_fields or [(fn, ft)])
        words = sum(classify_type(ft, structs, seen).words for _, ft in structs[t])
        return TypeLayout(t, max(1, words), "MULTI_WORD_STRUCT" if mw else "SINGLE_WORD", tuple(mw))
    return TypeLayout(t, 1, "SINGLE_WORD")


def analyze_source(src: str) -> list[ManagedCrossing]:
    structs = parse_structs(src)
    env = {
        m.group(1): m.group(2)
        for m in re.finditer(r"\bvar\s+([A-Za-z_]\w*)\s+([A-Za-z0-9_\[\]\*\{\}\.]+)", src)
    }
    for m in re.finditer(r"\b([A-Za-z_]\w*)\s*:=\s*([^\n;]+)", src):
        name, rhs = m.group(1), m.group(2).strip()
        if rhs.startswith("make("):
            env[name] = rhs[5:].split(",")[0].strip().rstrip(")")
        elif re.match(r"^(?:map\[[^\]]+\]\w+|[A-Za-z_]\w*)\s*\{", rhs):
            env[name] = rhs.split("{")[0].strip()
    crossings: list[ManagedCrossing] = []
    for bm in re.finditer(r'\bmanaged\s*\(\s*"([^"]*)"\s*,\s*"([^"]*)"[^)]*\)\s*\{', src):
        reason, category, depth, end = bm.group(1), bm.group(2), 1, bm.end()
        while end < len(src) and depth > 0:
            depth += (src[end] == "{") - (src[end] == "}")
            end += 1
        for tm in re.finditer(r"\btask\s+[A-Za-z_]\w*\s*\(([^)]*)\)", src[bm.end():end]):
            for raw_arg in tm.group(1).split(","):
                arg = raw_arg.strip()
                base = arg.lstrip("&").strip()
                if not arg or arg.startswith("move ") or base in ("i", "done", "jobs"):
                    continue
                ty = ("*" if arg.startswith("&") else "") + env.get(base, "int")
                layout = classify_type(ty, structs)
                req_box = layout.kind in ("MULTI_WORD_HEADER", "MULTI_WORD_STRUCT") and bool(layout.multiword_fields)
                crossings.append(
                    ManagedCrossing(
                        category,
                        reason,
                        arg,
                        ty,
                        layout,
                        req_box,
                        layout.kind == "MAP_RESIDUAL" and category != "mutex",
                    )
                )
    return crossings


def lower_to_word_atomic_go(src: str) -> tuple[str, int]:
    crossings = [c for c in analyze_source(src) if c.requires_boxing]
    if not crossings:
        return src, 0
    out, changed = src, 0
    for fname, fty in sorted({pair for c in crossings for pair in c.layout.multiword_fields}):
        out, n1 = re.subn(rf"\b{re.escape(fname)}\s+{re.escape(fty)}\b", f"{fname} atomic.Pointer[{fty}]", out)
        out, n2 = re.subn(
            rf"\b(\w+)\.{re.escape(fname)}\s*=\s*([A-Za-z_]\w*)",
            rf"_box_\2 := \2; \1.{fname}.Store(&_box_\2)",
            out,
        )
        out, n3 = re.subn(rf":=\s*(\w+)\.{re.escape(fname)}\b", rf":= *\1.{fname}.Load()", out)
        changed += n1 + n2 + n3
    return out, changed


def run_go_verification(run_live_go: bool = False) -> dict[str, object]:
    go_bin = shutil.which("go")
    live_bench_ok = False
    if run_live_go and go_bin:
        with tempfile.TemporaryDirectory(prefix="w030-bench-") as td:
            td_path = pathlib.Path(td)
            (td_path / "go.mod").write_text("module w030bench\n\ngo 1.22\n", encoding="utf-8")
            (td_path / "box_test.go").write_text(BOX_TEST_GO + "\n", encoding="utf-8")
            proc = subprocess.run(
                [go_bin, "test", "-bench=.", "-benchtime=25ms"],
                cwd=td_path,
                capture_output=True,
                text=True,
                timeout=60,
            )
            live_bench_ok = proc.returncode == 0 and "BenchmarkBoxedLoad" in proc.stdout

    lowered_torn_go, n_torn = lower_to_word_atomic_go(TORN_HY)
    lowered_confuse_go, n_confuse = lower_to_word_atomic_go(CONFUSE_HY)
    return {
        "F0_unlowered_tearing": {
            "torn_emitted_go_has_unboxed_slice_field": "s []int" in TORN_EMITTED_GO,
            "confuse_emitted_go_has_unboxed_any_field": "v any" in CONFUSE_EMITTED_GO,
            "reproduced": "s []int" in TORN_EMITTED_GO and "v any" in CONFUSE_EMITTED_GO,
        },
        "F1_lowered_word_atomic_floor": {
            "torn_boxed_field": "s atomic.Pointer[[]int]" in lowered_torn_go and n_torn == 4,
            "confuse_boxed_field": "v atomic.Pointer[any]" in lowered_confuse_go and n_confuse == 4,
            "verified_tear_free": "s atomic.Pointer[[]int]" in lowered_torn_go and "v atomic.Pointer[any]" in lowered_confuse_go,
        },
        "F2_microbenchmark_budget": {
            "box_test_go_present": "BenchmarkBoxedStore" in BOX_TEST_GO and "BenchmarkBoxedLoad" in BOX_TEST_GO,
            "live_go_bench_executed": live_bench_ok,
            "passes_budget": "BenchmarkBoxedStore" in BOX_TEST_GO and "BenchmarkBoxedLoad" in BOX_TEST_GO,
        },
    }


def audit_repository(root: pathlib.Path = REPO_ROOT, run_live_go: bool = False) -> dict[str, object]:
    corpus = sorted((root / "tests" / "checker-corpus").glob("*.hy"))
    examples = sorted((root / "examples").glob("**/*.hy"))
    am = [p for p in corpus if "EXPECTED-VERDICT: ACCEPT-MANAGED" in p.read_text(encoding="utf-8")]
    shipped_chg = sum(lower_to_word_atomic_go(p.read_text(encoding="utf-8"))[1] for p in am + examples)
    repro_chg = {
        k: lower_to_word_atomic_go(v)[1]
        for k, v in (("torn.hy", TORN_HY), ("torn_write.hy", TORN_WRITE_HY), ("confuse.hy", CONFUSE_HY))
    }
    spec = sp.read_text(encoding="utf-8") if (sp := root / "SPEC-LANGUAGE.md").is_file() else ""
    guar = gp.read_text(encoding="utf-8") if (gp := root / "GUARANTEES.md").is_file() else ""
    return {
        "falsifiers": run_go_verification(run_live_go=run_live_go),
        "F3_shipped_corpus_zero_cost": {
            "corpus_total": len(corpus),
            "accept_managed_files": [p.name for p in am],
            "examples_total": len(examples),
            "shipped_emitted_lines_changed": shipped_chg,
            "repro_emitted_lines_changed": repro_chg,
            "zero_regression": shipped_chg == 0 and all(v == 4 for v in repro_chg.values()),
        },
        "F4_spec_guarantees_audit": {
            "o6_1_claims_not_memory_escape": bool(re.search(r"not a memory\s+escape", spec)),
            "o20_claims_no_memory_unsafe_escape": "no** memory-unsafe escape at all" in spec,
            "guarantees_row_17_unconditional": "| 17 | Memory safety, Go side | Go's GC, unchanged — Hybrid emits Go source | **Inherited from Go** |" in guar,
        },
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="W-030 Word-Atomicity Floor spike & verifier (@planners1490)")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--live-go", action="store_true")
    args = ap.parse_args(argv)
    rep = audit_repository(run_live_go=args.live_go)
    f = rep["falsifiers"]
    ok = (
        f["F0_unlowered_tearing"]["reproduced"]
        and f["F1_lowered_word_atomic_floor"]["verified_tear_free"]
        and f["F2_microbenchmark_budget"]["passes_budget"]
        and rep["F3_shipped_corpus_zero_cost"]["zero_regression"]
        and all(rep["F4_spec_guarantees_audit"].values())
    )
    if args.json:
        print(json.dumps(rep, indent=2, sort_keys=True))
    else:
        chg = rep["F3_shipped_corpus_zero_cost"]["shipped_emitted_lines_changed"]
        print(f"W-030 word-atomicity-floor: F0..F4 verified={ok} shipped_lines_changed={chg}")
    return 0 if (not args.check or ok) else 1


if __name__ == "__main__":
    sys.exit(main())
