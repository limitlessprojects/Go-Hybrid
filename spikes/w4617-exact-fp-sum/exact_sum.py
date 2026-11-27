"""W-4617 (#4617): Exact floating-point summation with worker-count independence.

Implements an opaque exact accumulator that lets worker counts change
without altering a single bit of the final floating-point sum.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from fractions import Fraction
from typing import List, Optional, Tuple
import math
import struct


@dataclass
class ExactAccumulator:
    """Opaque exact partial sum using integer-scaled units (units = mantissa << 1074).
    Cannot be observed as a float until finalize() is called."""
    _units: int = 0
    _finalized: bool = False

    def add(self, value: float) -> None:
        if self._finalized:
            raise RuntimeError("HYB-E7001: accumulator already finalized; affine reuse forbidden")
        if math.isnan(value) or math.isinf(value):
            raise ValueError("HYB-E7002: nonfinite input rejected from exact summation region")
        numerator, denominator = value.as_integer_ratio()
        self._units += numerator << (1074 - (denominator.bit_length() - 1))

    def merge(self, other: "ExactAccumulator") -> "ExactAccumulator":
        """Merges two opaque accumulators into one. Both inputs are consumed."""
        if self._finalized or other._finalized:
            raise RuntimeError("HYB-E7001: accumulator already finalized; affine reuse forbidden")
        result = ExactAccumulator()
        result._units = self._units + other._units
        self._finalized = True
        other._finalized = True
        return result

    def finalize(self) -> float:
        """Converts the exact integer sum to binary64, rounding once. Consumes the accumulator."""
        if self._finalized:
            raise RuntimeError("HYB-E7001: accumulator already finalized; affine reuse forbidden")
        self._finalized = True
        try:
            f = float(Fraction(self._units, 1 << 1074))
        except OverflowError as exc:
            raise OverflowError("HYB-E7003: exact sum overflows binary64 on final conversion") from exc
        if math.isinf(f):
            raise OverflowError("HYB-E7003: exact sum overflows binary64 on final conversion")
        # Ensure exact zero maps to +0.0
        if f == 0.0:
            return 0.0
        return f


def partitioned_exact_sum(values: List[float], num_workers: int) -> float:
    """Simulates a partitioned exact summation across num_workers workers."""
    chunks = [values[k::num_workers] for k in range(num_workers)]
    accumulators = []
    for chunk in chunks:
        acc = ExactAccumulator()
        for v in chunk:
            acc.add(v)
        accumulators.append(acc)

    # Merge tree (left fold for simplicity)
    while len(accumulators) > 1:
        merged = accumulators[0].merge(accumulators[1])
        accumulators = [merged] + accumulators[2:]

    return accumulators[0].finalize()


def ordinary_fold_sum(values: List[float]) -> float:
    """Ordinary left-fold floating-point sum (non-exact, ordering-dependent)."""
    total = 0.0
    for x in values:
        total += x
    return total
