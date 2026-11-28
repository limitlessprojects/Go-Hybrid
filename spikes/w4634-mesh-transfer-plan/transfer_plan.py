"""W-4634 (#4634): Transfer plan carrying write partition and conservation law.

Implements checked 1D mesh field transfer preserving cell integrals and disjoint writes.
"""

from __future__ import annotations
from fractions import Fraction
from typing import List, Tuple


def cells(edges: Tuple[int, ...]) -> List[Tuple[int, int]]:
    return list(zip(edges, edges[1:]))


def overlap(a: Tuple[int, int], b: Tuple[int, int]) -> int:
    return max(0, min(a[1], b[1]) - max(a[0], b[0]))


def row_plan(source: Tuple[int, ...], dest: Tuple[int, ...]) -> List[List[Fraction]]:
    return [
        [Fraction(overlap(a, b), b[1] - b[0]) for a in cells(source)]
        for b in cells(dest)
    ]


def validate_plan(source: Tuple[int, ...], dest: Tuple[int, ...], rows: List[List[Fraction]]) -> bool:
    source_cells, target_cells = cells(source), cells(dest)
    if len(rows) != len(target_cells):
        return False
    for b, row in zip(target_cells, rows):
        if len(row) != len(source_cells):
            return False
        for a, weight in zip(source_cells, row):
            if weight * (b[1] - b[0]) != overlap(a, b):
                return False
    return True


def run_partitioned(rows: List[List[Fraction]], density: List[int], workers: int) -> List[Fraction]:
    dst = [None] * len(rows)
    for worker in reversed(range(workers)):
        for j in range(worker, len(rows), workers):
            dst[j] = sum(w * v for w, v in zip(rows[j], density))
    return dst


def total_integral(edges: Tuple[int, ...], rho: List[any]) -> Fraction:
    return sum((b - a) * v for (a, b), v in zip(cells(edges), rho))


def cell_oracle(source: Tuple[int, ...], dest: Tuple[int, ...], rho: List[int]) -> List[Fraction]:
    raster = [v for (a, b), v in zip(cells(source), rho) for _ in range(b - a)]
    return [Fraction(sum(raster[a:b]), b - a) for a, b in cells(dest)]
