#!/usr/bin/env python3
"""Spike 4 analysis: medians for the dispatch benches + S1-S6 verdicts from
the matrix JSONL files. Reads files written by run.sh; prints verdicts.
House rule: thresholds were pre-registered in RESULTS.md BEFORE any run."""
import json
import re
import statistics
import sys
from pathlib import Path

WORK = Path(__file__).resolve().parent


def bench_medians(path):
    rows = {}
    if not path.exists():
        return rows
    text = path.read_text()
    for line in text.splitlines():
        m = re.match(r"(Benchmark\w+)-\d+\s+(\d+)\s+([\d.]+) ns/op(?:\s+(\d+) B/op\s+(\d+) allocs/op)?", line)
        if m:
            rows.setdefault(m.group(1), []).append((float(m.group(3)), int(m.group(4) or 0), int(m.group(5) or 0)))
    out = {}
    for k, v in rows.items():
        out[k] = (statistics.median(x[0] for x in v),
                  statistics.median(x[1] for x in v),
                  statistics.median(x[2] for x in v))
    return out


def load_cells(path):
    cells = []
    if not path.exists():
        return cells
    for line in path.read_text().splitlines():
        line = line.strip()
        if line:
            try:
                cells.append(json.loads(line))
            except json.JSONDecodeError:
                pass
    return cells


def load_cells_any(*names):
    for n in names:
        c = load_cells(WORK / n)
        if c:
            return c, n
    return [], None


def find(cells, mech, n, f, d):
    for c in cells:
        if c["mech"] == mech and c["n"] == n and c["frac_pct"] == f and c["d_ms"] == d:
            return c
    return None


def cell_display(c):
    if c is None:
        return "(missing)"
    if c.get("note", "").startswith("SKIPPED"):
        return f"[skipped: {c['note']}]"
    cap = " CAPPED" if c.get("capped") else ""
    return (f"wall={c['wall_s']:.2f}s{cap} done={c.get('done','-')}/{c['n']} "
            f"p99={c.get('p99_us',0):.0f}us canaryP99={c.get('canary_p99_us',0):.0f}us "
            f"thr={c.get('peak_threads',0)}")


def main():
    print("== dispatch bench medians ==")
    bm = bench_medians(WORK / "bench.sandbox.out")
    for k in sorted(bm):
        ns, by, al = bm[k]
        print(f"  {k:28s} {ns:10.2f} ns/op  {int(by):4d} B/op  {int(al):2d} allocs/op")

    cells, cells_path = load_cells_any("go/matrix.sandbox.jsonl", "matrix.sandbox.jsonl")
    worker, _ = load_cells_any("go/matrix.worker.jsonl", "matrix.worker.jsonl")
    print(f"\n== cells loaded: {cells_path}: {len(cells)}; worker={len(worker)} ==")

    # ---- S1: M1 starvation at (10k, 50%, 100ms)
    print("\n== S1: M1 naive starvation ==")
    m0 = find(cells, "M0", 10000, 50, 100)
    m1 = find(cells, "M1", 10000, 50, 100)
    print(f"  M0: {cell_display(m0)}")
    print(f"  M1: {cell_display(m1)}")
    if m0 and m1 and not m0.get("capped") and m1.get("wall_s"):
        ratio = m1["wall_s"] / m0["wall_s"]
        verdict = "PASS" if ratio >= 5 else "FAIL"
        print(f"  ratio M1/M0 = {ratio:.1f}x -> S1 {verdict} (>=5x pre-registered)")
        # expected floor: N*f*d / GOMAXPROCS
        exp = (10000 * 0.5 * 0.1) / m1.get("gomaxprocs", 2)
        print(f"  expected floor N*f*d/GOMAXPROCS = {exp:.0f}s; measured {m1['wall_s']:.1f}s")

    # ---- S2: M2b vs M0 across all shared uncapped cells
    print("\n== S2: M2b wall <= 1.5x M0 on every uncapped shared cell ==")
    bad = []
    for c in cells:
        if c["mech"] != "M2b" or c.get("capped") or c.get("note", "").startswith("SKIPPED"):
            continue
        b = find(cells, "M0", c["n"], c["frac_pct"], c["d_ms"])
        if not b or b.get("capped"):
            continue
        r = c["wall_s"] / b["wall_s"]
        if r > 1.5:
            bad.append((c["n"], c["frac_pct"], c["d_ms"], r))
        cp = c.get("canary_p99_us", 0) / max(b.get("canary_p99_us", 1), 1)
        if cp > 3:
            bad.append((c["n"], c["frac_pct"], c["d_ms"], f"canary {cp:.1f}x"))
    print(f"  violations: {bad if bad else 'none'} -> S2 {'FAIL' if bad else 'PASS'}")
    m2b0 = bm.get("BenchmarkM2bZero")
    if m2b0:
        print(f"  M2b d=0 pair cost: {m2b0[0]:.1f} ns (pre-registered <=100ns) -> "
              f"{'PASS' if m2b0[0] <= 100 else 'FAIL (see RESULTS: sandbox syscall-interception; worker re-measures; amortized for real >=1ms blocks)'}")

    # ---- S3: M2a characterization
    print("\n== S3: M2a lazy-handoff characterization ==")
    for d in (100,):
        a = find(cells, "M2a", 10000, 50, d)
        b = find(cells, "M0", 10000, 50, d)
        if a and b:
            r = a["wall_s"] / b["wall_s"]
            print(f"  (10k,50%,{d}ms) M2a/M0 = {r:.2f}x -> {'PASS' if r <= 1.5 else 'FAIL'} (<=1.5x)")
    for d in (1,):
        a = find(cells, "M2a", 10000, 50, d)
        b = find(cells, "M0", 10000, 50, d)
        if a and b:
            r = a["wall_s"] / b["wall_s"]
            print(f"  (10k,50%,{d}ms) M2a/M0 = {r:.2f}x (characterization; sysmon ~20ms retake)")

    # ---- S4: M3 full drain + latency overhead
    print("\n== S4: M3 pool ==")
    m3 = find(cells, "M3", 10000, 50, 100)
    b = find(cells, "M0", 10000, 50, 100)
    print(f"  (10k,50%,100ms): {cell_display(m3)}")
    pool_threads = 128  # pre-registered (concurrency-sized; see RESULTS.md)
    if m3 and b:
        bound = (10000 * 0.5 * 0.1) / pool_threads  # throughput bound
        ok_wall = m3["wall_s"] <= 1.5 * bound
        print(f"  wall {m3['wall_s']:.1f}s vs 1.5x bound {1.5*bound:.1f}s -> {'PASS' if ok_wall else 'FAIL'}")
    # latency overhead at the low-concurrency cells (pre-registered
    # operationalization: p99-d isolates dispatch+backoff where demand < pool)
    for d in (1, 100):
        c = find(cells, "M3", 1000, 1, d)
        if c and not c.get("capped"):
            lat99 = (c.get("p99_us", 0) / 1000.0) - d
            print(f"  (1k,1%,{d}ms) p99 latency overhead: {lat99:.2f} ms (<=5ms) -> {'PASS' if lat99 <= 5.0 else 'FAIL'}")
    # zero ticket loss at full drain on any uncapped M3 cell
    lost = [c for c in cells if c["mech"] == "M3" and not c.get("capped")
            and not c.get("note", "").startswith("SKIPPED") and c.get("done") != c["n"]]
    print(f"  M3 uncapped cells with done<n: {lost if lost else 'none'}")

    # ---- S5: thread accounting (two-tier reality: threads are BOUNDED by
    # design — syswin cap 384 + pool 128 — not proportional to blocked count)
    print("\n== S5: thread accounting ==")
    bad = []
    for c in cells:
        if c["mech"] in ("M2a", "M2b") and not c.get("capped") and not c.get("note", "").startswith("SKIPPED"):
            limit = c.get("base_threads", 0) + 384 + 128 + 32
            if c.get("peak_threads", 0) > limit:
                bad.append((c["n"], c["frac_pct"], c["d_ms"], c["peak_threads"], limit))
    print(f"  two-tier thread bound violations (base+384+128+32): {bad if bad else 'none'} -> S5 {'FAIL' if bad else 'PASS (bounded by design)'}")
    for n, f, d in ((10000, 50, 100),):
        c = find(cells, "M2b", n, f, d)
        if c:
            print(f"  (n={n},f={f}%,d={d}ms) M2b peak threads {c.get('peak_threads')} (base {c.get('base_threads')}) — lane-bounded, NOT blocked-count-proportional")

    # ---- worker cross-checks when present
    if worker:
        print("\n== worker tier (100k) highlights ==")
        for mech in ("M0", "M1", "M2b", "M3"):
            for f, d in ((10, 10), (50, 10)):
                c = find(worker, mech, 100000, f, d)
                if c:
                    print(f"  {mech} (100k,{f}%,{d}ms): {cell_display(c)}")
        m3 = find(worker, "M3", 100000, 50, 100)
        if m3:
            print(f"  M3 (100k,50%,100ms): {cell_display(m3)}")

    print("\n(analysis complete — S6 synthesis is written by the lead into RESULTS.md)")


if __name__ == "__main__":
    sys.exit(main())
