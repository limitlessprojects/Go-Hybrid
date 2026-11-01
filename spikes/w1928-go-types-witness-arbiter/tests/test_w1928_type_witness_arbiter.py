"""Unit tests for W-1928 (#1928): A go/types Witness that Arbitrates the Shape Oracle."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from type_witness_arbiter import ExprProbe, arbitrate_with_witness  # noqa: E402


class TestW1928TypeWitnessArbiter(unittest.TestCase):
    def setUp(self):
        self.probes = [
            ExprProbe("c01.hy", (10, 20), "Unknown", "[]int"),
            ExprProbe("c02.hy", (30, 40), "Unknown", "map[string]bool"),
            ExprProbe("c03.hy", (50, 60), "Unknown", "Point"),
            ExprProbe("c04.hy", (70, 80), "Unknown", "Unknown", at_spawn_boundary=True),
            ExprProbe("c05.hy", (90, 99), "int", "int"),
        ] + [
            ExprProbe(f"drift_{i:02d}.hy", (i * 10, i * 10 + 5), "int", "[]byte", bug_ref="#1805" if i == 1 else "#1822")
            for i in range(1, 12)
        ]

    def test_01_arbitration_off_preserves_byte_identical_shape_verdicts(self):
        rep = arbitrate_with_witness(self.probes, arbitration_enabled=False)
        self.assertEqual(rep.unknown_recovery_ratio, 0.0)
        self.assertEqual(len(rep.disagreements), 0)
        self.assertEqual(rep.resolved_types[("c01.hy", (10, 20))], "Unknown")

    def test_02_unknown_recovery_exceeds_15_percent_threshold(self):
        rep = arbitrate_with_witness(self.probes, arbitration_enabled=True)
        # 3 of 4 Unknown positions recover concrete types -> 0.75 (>= 0.15)
        self.assertGreaterEqual(rep.unknown_recovery_ratio, 0.15)
        self.assertEqual(rep.unknown_recovery_ratio, 0.75)

    def test_03_surfaces_at_least_10_disagreements_including_1805_and_1822(self):
        rep = arbitrate_with_witness(self.probes, arbitration_enabled=True)
        self.assertGreaterEqual(len(rep.disagreements), 10)
        self.assertTrue(any("#1805" in d for d in rep.disagreements))
        self.assertTrue(any("#1822" in d for d in rep.disagreements))

    def test_04_hyb_e3005_still_rejects_all_three_fail_closed_spawn_boundaries(self):
        spawn_probes = [
            ExprProbe("s1.hy", (1, 5), "Unknown", "Unknown", at_spawn_boundary=True),
            ExprProbe("s2.hy", (6, 9), "Unknown", "[]byte", at_spawn_boundary=True),
            ExprProbe("s3.hy", (10, 15), "int", "map[string]int", at_spawn_boundary=True),
        ]
        rep = arbitrate_with_witness(spawn_probes, arbitration_enabled=True)
        self.assertEqual(len(rep.spawn_failures), 3)

    def test_05_emitter_and_checker_agree_on_every_resolved_type(self):
        rep = arbitrate_with_witness(self.probes, arbitration_enabled=True)
        self.assertTrue(rep.emitter_checker_parity)


if __name__ == "__main__":
    unittest.main()
