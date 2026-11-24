"""Tests for W-4723: Constant conversion overflow check for narrower integer types."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from narrow_overflow import (
    check_integer_constant_conversion,
    check_float_constant_conversion,
    validate_conversion_table,
    IntType,
)


class TestNarrowIntOverflow(unittest.TestCase):
    def test_int8_200_rejected_like_go(self):
        """int8(200) must be rejected — Go says 'constant 200 overflows int8'."""
        res = check_integer_constant_conversion(200, IntType.INT8)
        self.assertFalse(res.accepted)
        self.assertEqual(res.code, "E6006")
        self.assertIn("overflows int8", res.diagnostic)

    def test_int8_neg200_rejected_like_go(self):
        """int8(-200) must be rejected — Go says 'constant -200 overflows int8'."""
        res = check_integer_constant_conversion(-200, IntType.INT8)
        self.assertFalse(res.accepted)
        self.assertIn("overflows int8", res.diagnostic)

    def test_int8_boundary_values_accepted(self):
        """int8(-128) and int8(127) must both be accepted."""
        res_lo = check_integer_constant_conversion(-128, IntType.INT8)
        res_hi = check_integer_constant_conversion(127, IntType.INT8)
        self.assertTrue(res_lo.accepted)
        self.assertTrue(res_hi.accepted)

    def test_uint8_256_rejected(self):
        """uint8(256) must be rejected."""
        res = check_integer_constant_conversion(256, IntType.UINT8)
        self.assertFalse(res.accepted)
        self.assertIn("overflows uint8", res.diagnostic)

    def test_uint64_negative_rejected(self):
        """uint64(-1) must be rejected — already works in hyb (E6006)."""
        res = check_integer_constant_conversion(-1, IntType.UINT64)
        self.assertFalse(res.accepted)
        self.assertIn("overflows uint64", res.diagnostic)

    def test_int64_max_accepted(self):
        """int64(9223372036854775807) (max int64) must be accepted."""
        res = check_integer_constant_conversion(9223372036854775807, IntType.INT64)
        self.assertTrue(res.accepted)

    def test_int16_32768_rejected(self):
        """int16(32768) must be rejected — one past max int16."""
        res = check_integer_constant_conversion(32768, IntType.INT16)
        self.assertFalse(res.accepted)
        self.assertIn("overflows int16", res.diagnostic)

    def test_float_truncation_rejected(self):
        """int8(200.9) must be rejected (not representable as integer)."""
        res = check_float_constant_conversion(200.9, IntType.INT8)
        self.assertFalse(res.accepted)
        self.assertIn("truncated", res.diagnostic)

    def test_full_conversion_table_matches_go(self):
        """All 11 cases from the issue table match expected Go behavior."""
        results = validate_conversion_table()
        for label, matched, diagnostic in results:
            self.assertTrue(matched, f"Conversion {label} did not match Go behavior: {diagnostic}")


if __name__ == "__main__":
    unittest.main()
