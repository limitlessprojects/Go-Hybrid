#!/usr/bin/env python3
"""Independent re-derivation of the ISSUE #4634 reference experiment.

Written from the *claims* in the issue text, not from the issue's code block.
The claims under test:

  C1  all pairs of meshes obtained by partitioning [0, 4] at integer interior
      points, source densities in {0, 1, 4}  ->  1536 mesh/data cases
  C2  workers in {1, 2, 3}                  ->  4608 simulated worker layouts
  C3  overlap-weighted averages agree with an independent unit-cell
      integration oracle on every output cell, and preserve the integral
  C4  the negative control (swapped rows of equal width) preserves the
      integral, produces wrong cells, and is rejected by the plan check
  C5  copying by cell index keeps the write schedule disjoint but changes the
      integral (the issue's [4,1] / [0,1,3] -> [0,2,3] example: 6 -> 9)

Design choices that make this an independent check rather than a transcription:
  * the plan is stored *sparse* (only non-zero overlaps), as a production 1D
    planner would, not as the dense matrix the reference builds;
  * two oracles are implemented and cross-checked against each other:
      OBS-A  exact analytic integration: cumulative piecewise-constant
             integral evaluated at mesh edges (no overlap formula at all)
      OBS-B  unit-raster integration (the issue's oracle; needs integer edges)
  * the conservation law is checked as an exact rational identity on the
    *total*, and separately as a per-target-cell identity against OBS-A.

Usage:  python model.py           (prints a self-check report, exit 0 on pass)
"""

from fractions import Fraction
from itertools import combinations, product

DOMAIN = (0, 4)
DENSITIES = (0, 1, 4)
WORKER_COUNTS = (1, 2, 3)


# ---------------------------------------------------------------- mesh basics
def cells(edges):
    """[(e0,e1), (e1,e2), ...] -- cell i is target/source index i."""
    return [(edges[i], edges[i + 1]) for i in range(len(edges) - 1)]


def all_meshes():
    """Every partition of [0,4] at a subset of the integer interior points."""
    interior = tuple(range(DOMAIN[0] + 1, DOMAIN[1]))  # (1, 2, 3)
    return [(DOMAIN[0], *cut, DOMAIN[1])
            for k in range(len(interior) + 1)
            for cut in combinations(interior, k)]


def overlap(a, b):
    return max(0, min(a[1], b[1]) - max(a[0], b[0]))


# ------------------------------------------------------------------- the plan
def build_plan(src, dst):
    """Sparse plan: rows[j] = [(src_index, weight)] for non-zero overlaps only.

    weight semantics: weight * (dst cell width) == overlap length.  A worker
    computing the target average multiplies the source densities by the weight
    and sums; nothing else.
    """
    rows = []
    for (lo, hi) in cells(dst):
        width = hi - lo
        row = []
        for i, (sa, sb) in enumerate(cells(src)):
            ov = overlap((sa, sb), (lo, hi))
            if ov:
                row.append((i, Fraction(ov, width)))
        rows.append(row)
    return rows


def dense(rows, nsrc):
    """Expand a sparse plan into the reference model's dense matrix."""
    out = []
    for row in rows:
        d = [Fraction(0)] * nsrc
        for i, w in row:
            d[i] = w
        out.append(d)
    return out


def validate(src, dst, rows, src_edges_ok=True):
    """Check the two constraints the plan is supposed to carry.

    (1) geometry:      weight * dst_width == overlap, for every entry
    (2) partition:     each dst cell is covered exactly once, i.e. the row's
                       overlaps sum to the dst cell width, src indices unique,
                       and there is exactly one row per dst cell
    (3) conservation:  every source cell's measure is fully accounted for
                       across rows (partition of unity in the other direction)
    """
    sc, dc = cells(src), cells(dst)
    if len(rows) != len(dc):
        return False, "row count != dst cell count"
    seen = set()
    # (1) + (2)
    for j, (b, row) in enumerate(zip(dc, rows)):
        width = b[1] - b[0]
        covered = 0
        idxs = set()
        for i, w in row:
            if not (0 <= i < len(sc)):
                return False, f"row {j}: src index {i} out of range"
            if i in idxs:
                return False, f"row {j}: src index {i} duplicated"
            idxs.add(i)
            ov = overlap(sc[i], b)
            if w * width != ov:
                return False, f"row {j}: weight*width {w*width} != overlap {ov}"
            if w <= 0 or w > 1:
                return False, f"row {j}: weight {w} outside (0,1]"
            covered += ov
        if covered != width:
            return False, f"row {j}: overlaps sum to {covered} != width {width}"
        seen.add(j)
    if len(seen) != len(dc):
        return False, "duplicate target rows"
    # (3) every source cell fully distributed
    for i, a in enumerate(sc):
        aw = a[1] - a[0]
        acc = Fraction(0)
        for j, (b, row) in enumerate(zip(dc, rows)):
            for k, w in row:
                if k == i:
                    acc += w * (b[1] - b[0])
        if acc != aw:
            return False, f"src cell {i}: distributed {acc} != width {aw}"
    return True, ""


# ------------------------------------------------------------------- schedule
def run_partitioned(rows, rho, workers):
    """Simulate the worker sweep.  Reversed order so a double write is visible:
    an overlapping claim would leave the *later* value, exactly as a racing
    writer would in a real task graph."""
    dst = [None] * len(rows)
    writes = []
    for worker in reversed(range(workers)):
        for j in range(worker, len(rows), workers):
            dst[j] = sum((w * rho[i] for i, w in rows[j]), Fraction(0))
            writes.append(j)
    return dst, writes


# --------------------------------------------------------------- the oracles
def oracle_analytic(src, dst, rho):
    """OBS-A: integrate the piecewise-constant field analytically.

    I(x) = cumulative integral of rho from src[0] to x, evaluated by walking the
    source cell that contains x (no overlap length is ever computed).  The
    average of the field over [a,b] is (I(b)-I(a))/(b-a).
    """
    edges = list(src)
    cum = [Fraction(0)]
    for i in range(len(edges) - 1):
        cum.append(cum[-1] + (edges[i + 1] - edges[i]) * rho[i])

    def I(x):
        if x < edges[0] or x > edges[-1]:
            raise ValueError(f"{x} outside the source domain")
        lo, hi = 0, len(edges) - 1           # find i with edges[i] <= x <= edges[i+1]
        while lo < hi - 1:
            mid = (lo + hi) // 2
            if edges[mid] <= x:
                lo = mid
            else:
                hi = mid
        return cum[lo] + (x - edges[lo]) * rho[lo]

    return [Fraction(I(b) - I(a), b - a) for (a, b) in cells(dst)]


def oracle_raster(src, dst, rho):
    """OBS-B: the issue's oracle.  Raster a unit grid, average the raster."""
    raster = [v for (a, b), v in zip(cells(src), rho) for _ in range(b - a)]
    return [Fraction(sum(raster[a:b]), b - a) for (a, b) in cells(dst)]


def total(edges, rho):
    return sum(((b - a) * v for (a, b), v in zip(cells(edges), rho)), Fraction(0))


def overlap_graph_is_forest(src, dst):
    """The bipartite overlap graph of two partitions of the same domain is a forest
    (it splits into one tree per shared boundary).  Each component is a tree, so the
    transport matrix with fixed row and column margins is unique -- i.e. the geometry
    alone pins the weights down, and no compensating perturbation can exist.
    Check it instead of asserting it."""
    n, m = len(src) - 1, len(dst) - 1
    edges = [(i, n + j)
             for i, x in enumerate(cells(src))
             for j, y in enumerate(cells(dst)) if overlap(x, y)]
    parent = list(range(n + m))
    cycles = 0

    def find(u):
        while parent[u] != u:
            parent[u] = parent[parent[u]]
            u = parent[u]
        return u

    for u, v in edges:
        ru, rv = find(u), find(v)
        if ru == rv:
            cycles += 1
        else:
            parent[ru] = rv
    comps = len({find(u) for u in range(n + m)})
    return cycles == 0 and len(edges) == (n + m) - comps


# ------------------------------------------------------------------ self-checks
def check_counts(meshes):
    """C1/C2: are 1536 and 4608 the right numbers for the stated enumeration?"""
    pairs = len(meshes) ** 2
    data_cases = sum(3 ** (len(s) - 1) for s in meshes for _ in meshes)
    layouts = sum(3 ** (len(s) - 1) * len(WORKER_COUNTS) for s in meshes for _ in meshes)
    return pairs, data_cases, layouts


def main():
    meshes = all_meshes()
    pairs, expect_cases, expect_layouts = check_counts(meshes)
    print(f"meshes               : {len(meshes)}  {meshes}")
    print(f"mesh pairs           : {pairs}")
    print(f"mesh/data cases      : {expect_cases}   (issue claims 1536)")
    print(f"worker layouts       : {expect_layouts}   (issue claims 4608)")

    cases = layouts = 0
    oracle_disagreements = 0
    for src in meshes:
        for dst in meshes:
            rows = build_plan(src, dst)
            ok, why = validate(src, dst, rows)
            assert ok, f"plan rejected by its own check: {why}"
            for rho in product(DENSITIES, repeat=len(src) - 1):
                obs_a = oracle_analytic(src, dst, rho)
                obs_b = oracle_raster(src, dst, rho)
                if obs_a != obs_b:
                    oracle_disagreements += 1
                for workers in WORKER_COUNTS:
                    out, writes = run_partitioned(rows, rho, workers)
                    assert out == obs_a, (src, dst, rho, workers, out, obs_a)
                    assert out == obs_b
                    assert total(dst, out) == total(src, rho)
                    assert sorted(writes) == list(range(len(rows))), "layout coverage"
                    assert len(writes) == len(set(writes)), "double write"
                    layouts += 1
                cases += 1
    print(f"cases executed       : {cases}")
    print(f"layouts executed     : {layouts}")
    print(f"oracle A vs oracle B : {oracle_disagreements} disagreements")
    print(f"plan-vs-oracle       : 0 disagreements (assertions above)")

    # ---- C4: swapped rows of equal width, same mesh
    mesh = (0, 1, 2)
    swapped = [[(0, Fraction(0)), (1, Fraction(1))],  # row 0 = src 1
               [(0, Fraction(1)), (1, Fraction(0))]]  # row 1 = src 0
    wrong, _ = run_partitioned(swapped, [1, 4], 2)
    t_before, t_after = total(mesh, [1, 4]), total(mesh, wrong)
    ok, why = validate(mesh, mesh, swapped)
    print(f"\n[N1] swapped rows: out={wrong} oracle={oracle_analytic(mesh, mesh, [1, 4])} "
          f"integral_before={t_before} integral_after={t_after} "
          f"preserved={t_before == t_after} rejected={not ok} ({why})")
    assert t_before == t_after == 5
    assert wrong != oracle_analytic(mesh, mesh, [1, 4])
    assert not ok

    # ---- C5: the issue's headline example, copy by index
    s, d = (0, 1, 3), (0, 2, 3)
    rho = [4, 1]
    by_index = [[(0, Fraction(1)), (1, Fraction(0))], [(0, Fraction(0)), (1, Fraction(1))]]
    copied, writes = run_partitioned(by_index, rho, 2)
    weighted, _ = run_partitioned(build_plan(s, d), rho, 2)
    ok_idx, why_idx = validate(s, d, by_index)
    ok_w, _ = validate(s, d, build_plan(s, d))
    print(f"[N2] {s} -> {d} rho={rho}")
    print(f"     copy by index    : {copied}  integral {total(s, rho)} -> {total(d, copied)}"
          f"  writes={sorted(writes)} disjoint={len(writes)==len(set(writes))}"
          f"  plan rejected={not ok_idx} ({why_idx})")
    print(f"     overlap weighted : {weighted}  integral {total(s, rho)} -> {total(d, weighted)}"
          f"  plan rejected={not ok_w}")
    assert copied == [4, 1] and total(d, copied) == 9
    assert weighted == [Fraction(5, 2), Fraction(1)] and total(d, weighted) == 6
    assert not ok_idx and ok_w

    # ---- N3: perturb one entry inside a row, keeping that row's measure
    dcells = cells(d)
    bad_row = [[(0, Fraction(1, 4)), (1, Fraction(3, 4))],   # 1/4*2 + 3/4*2 = 2 = width
               [(0, Fraction(0)), (1, Fraction(1))]]
    ok3, why3 = validate(s, d, bad_row)
    out3, _ = run_partitioned(bad_row, rho, 1)
    row_measure = [sum(w * (b[1] - b[0]) for _, w in row) for row, b in zip(bad_row, dcells)]
    print(f"[N3] in-row perturbation: row measures {row_measure} (dst widths 2,1); "
          f"out={out3} integral {total(s, rho)} -> {total(d, out3)} "
          f"rejected={not ok3} ({why3})")
    assert row_measure == [2, 1]        # the write partition still looks legal
    assert not ok3                      # the geometry check still rejects it
    print("     (the total happens to break too -- see N4: in 1D the margins are not independent)")

    # ---- N4: is the weight matrix actually pinned down by the margins?
    tree, pairs_seen = 0, 0
    for a in meshes:
        for b in meshes:
            pairs_seen += 1
            if overlap_graph_is_forest(a, b):
                tree += 1
    print(f"[N4] overlap graph is a forest for {tree}/{pairs_seen} mesh pairs "
          f"-> row+column margins uniquely determine the weights")
    assert tree == pairs_seen

    # ---- N5: two workers claiming the same row
    def overlapping_sweep(rows, rho, claims):
        written = {}
        for j in claims:
            if j in written:
                return written, f"row {j} written twice"
            written[j] = sum((w * rho[i] for i, w in rows[j]), Fraction(0))
        return written, ""

    _, why5 = overlapping_sweep(build_plan(s, d), rho, [0, 1, 0])
    print(f"[N5] overlapping worker claims: {why5}")
    assert why5

    print("\nSELF-CHECK PASSED")


if __name__ == "__main__":
    main()

