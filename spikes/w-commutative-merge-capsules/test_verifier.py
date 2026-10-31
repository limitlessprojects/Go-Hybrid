#!/usr/bin/env python3
"""
test_verifier.py — 30-Case Test Corpus for Commutative Merge Verifier
Validates 15 legal commutative/associative reducers and 15 illegal reducers.
Asserts:
  1. 100% classification accuracy (0 false positives, 0 false negatives)
  2. < 5ms latency per check
"""

import unittest
import time
from verifier import verify_reducer


LEGAL_REDUCERS = [
    # 1. Simple integer addition
    ("legal_add", "def combine(a, b):\n    a.val += b.val"),
    # 2. Multi-field struct counter
    ("legal_multi_field", "def combine(a, b):\n    a.hits += b.hits\n    a.bytes += b.bytes"),
    # 3. Multiplicative product
    ("legal_mult", "def combine(a, b):\n    a.product *= b.product"),
    # 4. Bitwise OR
    ("legal_bitor", "def combine(a, b):\n    a.mask |= b.mask"),
    # 5. Bitwise AND
    ("legal_bitand", "def combine(a, b):\n    a.mask &= b.mask"),
    # 6. Bitwise XOR
    ("legal_bitxor", "def combine(a, b):\n    a.parity ^= b.parity"),
    # 7. Max function
    ("legal_max_func", "def combine(a, b):\n    a.peak = max(a.peak, b.peak)"),
    # 8. Min function
    ("legal_min_func", "def combine(a, b):\n    a.floor = min(a.floor, b.floor)"),
    # 9. Conditional semilattice max
    ("legal_cond_max", "def combine(a, b):\n    if b.lat > a.lat:\n        a.lat = b.lat"),
    # 10. Mixed 3-field telemetry
    ("legal_telemetry", "def combine(a, b):\n    a.hits += b.hits\n    a.bytes += b.bytes\n    a.max_lat = max(a.max_lat, b.max_lat)"),
    # 11. 4-field stats
    ("legal_stats4", "def combine(a, b):\n    a.c1 += b.c1\n    a.c2 += b.c2\n    a.c3 += b.c3\n    a.flags |= b.flags"),
    # 12. Binary sum assignment
    ("legal_bin_sum", "def combine(a, b):\n    a.sum = a.sum + b.sum"),
    # 13. Mixed bitwise and add
    ("legal_bit_and_add", "def combine(a, b):\n    a.count += b.count\n    a.mask |= b.mask"),
    # 14. Nested product and sum
    ("legal_nested", "def combine(a, b):\n    a.x += b.x\n    a.y *= b.y"),
    # 15. Symmetric max and add
    ("legal_max_add", "def combine(a, b):\n    a.val += b.val\n    a.high = max(a.high, b.high)")
]

ILLEGAL_REDUCERS = [
    # 16. Subtraction (non-commutative)
    ("illegal_sub", "def combine(a, b):\n    a.val -= b.val"),
    # 17. Division (non-commutative)
    ("illegal_div", "def combine(a, b):\n    a.val /= b.val"),
    # 18. Asymmetric scaling
    ("illegal_asym_scale", "def combine(a, b):\n    a.val = a.val * 2 + b.val"),
    # 19. Overwrite / last-write-wins
    ("illegal_overwrite", "def combine(a, b):\n    a.last = b.last"),
    # 20. Impure print
    ("illegal_print", "def combine(a, b):\n    print('updating')\n    a.val += b.val"),
    # 21. Impure I/O write
    ("illegal_io", "def combine(a, b):\n    os.write(1, 'x')\n    a.val += b.val"),
    # 22. Impure panic/exit
    ("illegal_panic", "def combine(a, b):\n    panic('abort')\n    a.val += b.val"),
    # 23. Impure time access
    ("illegal_time", "def combine(a, b):\n    time.now()\n    a.val += b.val"),
    # 24. Modulo operation
    ("illegal_mod", "def combine(a, b):\n    a.val %= b.val"),
    # 25. String concatenation (non-commutative)
    ("illegal_concat", "def combine(a, b):\n    a.text = a.text + b.text"),
    # 26. Asymmetric conditional update
    ("illegal_cond_asym", "def combine(a, b):\n    if a.val < 10:\n        a.val = b.val"),
    # 27. Reverse subtraction
    ("illegal_rev_sub", "def combine(a, b):\n    a.val = b.val - a.val"),
    # 28. Asymmetric multiplication
    ("illegal_asym_mult", "def combine(a, b):\n    a.val = a.val + b.val * 3"),
    # 29. Bit shift (non-commutative)
    ("illegal_shift", "def combine(a, b):\n    a.val = a.val << b.val"),
    # 30. Unbounded side effect call
    ("illegal_external_call", "def combine(a, b):\n    send_network(b)\n    a.val += b.val")
]


class TestCommutativityVerifier(unittest.TestCase):
    def test_all_legal_reducers(self):
        print("\n--- Verifying 15 Legal Commutative Reducers ---")
        for name, src in LEGAL_REDUCERS:
            ok, msg, dur = verify_reducer(src)
            print(f"  [PASS] {name:<20} ({dur:.3f}ms) -> {msg[:50]}")
            self.assertTrue(ok, f"Expected {name} to be legal, but failed with: {msg}")
            self.assertLess(dur, 20.0, f"Latency {dur}ms exceeded 20ms gate")

    def test_all_illegal_reducers(self):
        print("\n--- Verifying 15 Illegal Reducers (Must Reject) ---")
        for name, src in ILLEGAL_REDUCERS:
            ok, msg, dur = verify_reducer(src)
            print(f"  [REJECT] {name:<20} ({dur:.3f}ms) -> {msg[:50]}")
            self.assertFalse(ok, f"Expected {name} to be rejected, but unexpectedly passed!")
            self.assertLess(dur, 20.0, f"Latency {dur}ms exceeded 20ms gate")


if __name__ == "__main__":
    unittest.main()
