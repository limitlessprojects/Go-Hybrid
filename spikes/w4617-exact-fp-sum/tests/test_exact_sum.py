"""Tests for W-4617: Exact FP summation with worker-count independence."""

import os
import sys
import math
import random
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from exact_sum import (
    ExactAccumulator,
    partitioned_exact_sum,
    ordinary_fold_sum,
)


class TestExactSum(unittest.TestCase):
    def test_cancellation_dataset_exact_across_worker_counts(self):
        """[1e16, 1, -1e16]*31: exact sum matches rational oracle for 1,2,3,7,16 workers."""
        values = [1e16, 1.0, -1e16] * 31
        from fractions import Fraction
        oracle = float(sum(map(Fraction, values), Fraction()))
        for workers in (1, 2, 3, 7, 16):
            result = partitioned_exact_sum(values, workers)
            self.assertEqual(result.hex(), oracle.hex(),
                f"Mismatch with {workers} workers: {result.hex()} != {oracle.hex()}")

    def test_subnormal_dataset_exact_across_worker_counts(self):
        """Subnormal values: exact sum is worker-independent."""
        tiny = math.ulp(0.0)
        values = [tiny, tiny, -tiny] * 31
        from fractions import Fraction
        oracle = float(sum(map(Fraction, values), Fraction()))
        for workers in (1, 2, 3, 7, 16):
            result = partitioned_exact_sum(values, workers)
            self.assertEqual(result.hex(), oracle.hex())

    def test_wide_exponent_range_exact(self):
        """Wide exponent range: [2^900, -2^900, 2^-60, -0.0]*23."""
        values = [2.0**900, -(2.0**900), 2.0**-60, -0.0] * 23
        from fractions import Fraction
        oracle = float(sum(map(Fraction, values), Fraction()))
        for workers in (1, 2, 3, 7):
            result = partitioned_exact_sum(values, workers)
            self.assertEqual(result.hex(), oracle.hex())

    def test_ordinary_fold_disagrees_with_exact(self):
        """Ordinary left-fold sum disagrees with exact oracle on cancellation data."""
        values = [1e16, 1.0, -1e16] * 31
        from fractions import Fraction
        oracle = float(sum(map(Fraction, values), Fraction()))
        folded = ordinary_fold_sum(values)
        self.assertNotEqual(folded.hex(), oracle.hex(),
            "Ordinary fold should disagree due to intermediate rounding")

    def test_nonfinite_input_rejected_with_e7002(self):
        """NaN and Inf inputs are rejected with HYB-E7002."""
        acc = ExactAccumulator()
        with self.assertRaises(ValueError) as cm:
            acc.add(float('nan'))
        self.assertIn("HYB-E7002", str(cm.exception))

        acc2 = ExactAccumulator()
        with self.assertRaises(ValueError) as cm:
            acc2.add(float('inf'))
        self.assertIn("HYB-E7002", str(cm.exception))

    def test_finalized_accumulator_reuse_rejected_with_e7001(self):
        """Affine reuse of a finalized accumulator is rejected with HYB-E7001."""
        acc = ExactAccumulator()
        acc.add(1.0)
        acc.finalize()
        with self.assertRaises(RuntimeError) as cm:
            acc.add(2.0)
        self.assertIn("HYB-E7001", str(cm.exception))

    def test_exact_zero_maps_to_positive_zero(self):
        """Exact zero from cancellation maps to +0.0, not -0.0."""
        values = [1.0, -1.0]
        result = partitioned_exact_sum(values, 2)
        self.assertEqual(result, 0.0)
        self.assertFalse(math.copysign(1.0, result) < 0, "Result should be +0.0")


if __name__ == "__main__":
    unittest.main()
