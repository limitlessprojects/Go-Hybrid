"""Reproduce every number quoted in RESULTS.md.

Usage: python3 spikes/w-orientation-authority-4642/measure.py
"""

from __future__ import annotations

import pathlib
import statistics
import sys
import time

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import orientation_authority as oa  # noqa: E402
import witness_checker as wc  # noqa: E402


def timeit(fn, repeats: int = 5) -> float:
    """Median wall time per call, in microseconds."""
    samples = []
    for _ in range(repeats):
        start = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - start) * 1e6)
    return statistics.median(samples)


def _swallow(source: str) -> None:
    """Time a whole check even when the case is an expected rejection."""
    try:
        wc.check_source(source)
    except wc.CheckError:
        pass


def predicate_tables() -> None:
    families = [
        ("random, |x| <= 2^26", oa.family_random(10_000, 1 << 26, 1729)),
        ("random, |x| <= 2^49", oa.family_random(10_000, 1 << 49, 1729)),
        ("unit determinant, n = 2^27..2^49", oa.family_unit_determinant()),
        ("collinear / repeated", oa.family_degenerate()),
    ]
    print(f"{'family':<34}{'cases':>7}{'naive_wrong':>13}{'exact_fallbacks':>16}{'filtered_wrong':>16}")
    for label, cases in families:
        row = oa.measure(label, cases)
        print(
            f"{label:<34}{row['cases']:>7}{row['naive_wrong']:>13}"
            f"{row['fallbacks']:>16}{row['filtered_wrong']:>16}"
        )


def timing() -> None:
    cases = oa.family_random(20_000, 1 << 26, 4242)
    naive_us = timeit(lambda: [oa.naive(*t) for t in cases]) / len(cases)
    classify_us = timeit(lambda: [oa.classify(*t) for t in cases]) / len(cases)
    print(f"\npredicate: naive {naive_us:.4f} us/op   filtered+exact {classify_us:.4f} us/op "
          f"({classify_us / naive_us:.2f}x)")

    certifier = oa.Certifier()
    mesh = oa.Mesh(1, [(0, 0), (10, 0), (0, 10)])
    witness_us = timeit(lambda: [certifier.observe(mesh, (0, 1, 2)) for _ in range(20_000)]) / 20_000
    print(f"authority: observe+mint+digest {witness_us:.4f} us/witness")

    w = certifier.observe(mesh, (0, 1, 2))
    commit_us = timeit(lambda: [oa.Mesh(1, [(0, 0), (10, 0), (0, 10)]).commit((0, 1, 2), w) for _ in range(5_000)]) / 5_000
    print(f"authority: commit gate {commit_us:.4f} us/commit (incl. mesh construction)")


def checker_tables() -> None:
    files = sorted((HERE / "cases").glob("*.hyc"))
    print(f"\n{'case':<34}{'verdict':<10}{'code':<12}{'us/case':>9}")
    total = 0.0
    for path in files:
        source = path.read_text(encoding="utf-8")
        header = source.splitlines()[0].replace("#", "").split()
        expected = header[1]
        code = header[2] if len(header) > 2 else ""
        per_case = timeit(lambda: _swallow(source), repeats=7)
        total += per_case
        try:
            wc.check_source(source)
            verdict = "accept"
            code = ""
        except wc.CheckError as err:
            verdict = "reject"
            code = err.code
        status = "ok" if verdict == expected else "MISMATCH"
        print(f"{path.name:<34}{verdict:<10}{code:<12}{per_case:>9.1f}  {status}")
    print(f"\nchecker total over {len(files)} cases: {total:.1f} us "
          f"(mean {total / len(files):.1f} us/case)")


def main() -> None:
    predicate_tables()
    timing()
    checker_tables()
    print("\nNOTE: absolute timings are single-process CPython 3.12 numbers on this host. "
          "The proposal asks for a cost signal, not a workload benchmark; treat the ratio as the signal.")


if __name__ == "__main__":
    main()
