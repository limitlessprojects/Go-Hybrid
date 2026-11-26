"""Tests for W-4787: Provably Minimal Undo Record for Lossy Reductions."""

import os
import sys
import unittest
from itertools import product

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from undo_codec import (
    LossyReductionEngine,
    rank_flags,
    unrank_flags,
    witness_bits_needed,
    comb,
)


class TestMinimalUndoRecord(unittest.TestCase):
    def setUp(self):
        self.engine = LossyReductionEngine(n=8)

    def test_exhaustive_256_roundtrip_recovery(self):
        """All 256 8-bit inputs round-trip exactly with 0 mismatches."""
        for pattern in product([False, True], repeat=8):
            input_flags = list(pattern)
            count, record = self.engine.forward_reduce(input_flags)
            reconstructed = self.engine.reverse_reconstruct(count, record)
            self.assertEqual(input_flags, reconstructed)

    def test_theoretical_witness_bits_bounds(self):
        """Validates exact witness bit bounds cited in the proposal."""
        # For n=8:
        # k=0: C(8,0) = 1 -> 0 bits
        # k=1: C(8,1) = 8 -> 3 bits
        # k=4: C(8,4) = 70 -> 7 bits
        self.assertEqual(witness_bits_needed(8, 0), 0)
        self.assertEqual(witness_bits_needed(8, 1), 3)
        self.assertEqual(witness_bits_needed(8, 4), 7)
        self.assertEqual(witness_bits_needed(8, 8), 0)

    def test_falsifier_out_of_bounds_rank_rejected_with_e8001(self):
        """Attempting to unrank with rank >= C(n, k) must raise HYB-E8001."""
        # For n=8, k=1: C(8,1) = 8. Valid ranks are 0..7. Rank 8 must fail.
        with self.assertRaises(ValueError) as cm:
            unrank_flags(8, 1, 8)
        self.assertIn("HYB-E8001", str(cm.exception))
        self.assertIn("Rank out of bounds", str(cm.exception))

    def test_negative_rank_rejected(self):
        """Negative rank must raise HYB-E8001."""
        with self.assertRaises(ValueError) as cm:
            unrank_flags(8, 2, -1)
        self.assertIn("HYB-E8001", str(cm.exception))

    def test_bit_savings_over_raw_snapshot(self):
        """Witness bits are strictly less than raw 8-bit snapshot for all k in [0, 8]."""
        for k in range(9):
            bits = witness_bits_needed(8, k)
            self.assertLessEqual(bits, 7)  # Peak is 7 bits at k=4, saving at least 1 bit (or 8 bits at k=0,8)


if __name__ == "__main__":
    unittest.main()
