"""Unit tests for W-2015 (#2015): O18 GoBench Spawn Census & O4.0 Closure/Method-Value Prover."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from gobench_spawn_prover import (  # noqa: E402
    SpawnSite,
    build_reference_gobench_35,
    evaluate_spawn_site,
    summarize_gobench_census,
)


class TestW2015GoBenchSpawnProver(unittest.TestCase):
    def test_01_gobench_35_kernel_census_matches_945_tripwire(self):
        kernels = build_reference_gobench_35()
        summary = summarize_gobench_census(kernels)
        self.assertEqual(summary["total_kernels"], 35)
        self.assertEqual(summary["spawn_bearing_kernels"], 34)
        self.assertEqual(summary["total_spawns"], 73)
        self.assertEqual(summary["plain_fn_spawns"], 4)
        self.assertEqual(summary["method_spawns"], 17)
        self.assertEqual(summary["closure_spawns"], 52)
        self.assertEqual(summary["baseline_unprovable_spawns"], 69)
        self.assertAlmostEqual(summary["baseline_unprovable_ratio"], 0.945, places=3)
        self.assertEqual(summary["zero_provable_baseline_kernels"], 30)
        self.assertTrue(summary["o18_tripwire_fired"])

    def test_02_etcd_4876_both_spawns_fail_baseline_hyb_e3005(self):
        kernels = build_reference_gobench_35()
        v1, v2 = [evaluate_spawn_site(s) for s in kernels["etcd/4876"]]
        self.assertFalse(v1.baseline_o40_provable)
        self.assertEqual(v1.baseline_code, "HYB-E3005_UNPROVABLE_CALLEE")
        self.assertTrue(v1.extended_o40_provable)
        self.assertFalse(v2.baseline_o40_provable)
        self.assertEqual(v2.extended_code, "HYB-E3001_CAPTURED_RACE")
        self.assertTrue(v2.requires_managed_hatch)

    def test_03_seven_single_spawn_kernels_5_of_7_lift_cleanly_under_o40_ext(self):
        single_spawns = [
            SpawnSite("k1", "method_value", receiver_sendable=True),
            SpawnSite("k2", "method_value", receiver_sendable=True),
            SpawnSite("k3", "closure", captures_sendable=True),
            SpawnSite("k4", "closure", captures_sendable=True),
            SpawnSite("k5", "closure", captures_sendable=True),
            SpawnSite("k6", "closure", captures_sendable=False, mutates_outer_alias=True),
            SpawnSite("k7", "method_value", receiver_sendable=False),
        ]
        verdicts = [evaluate_spawn_site(s) for s in single_spawns]
        self.assertEqual(sum(1 for v in verdicts if v.baseline_o40_provable), 0)
        self.assertEqual(sum(1 for v in verdicts if v.extended_o40_provable), 5)

    def test_04_plain_fn_with_shared_mutation_still_rejected(self):
        verdict = evaluate_spawn_site(SpawnSite("race_fn", "plain_fn", mutates_outer_alias=True))
        self.assertFalse(verdict.baseline_o40_provable)
        self.assertFalse(verdict.extended_o40_provable)
        self.assertEqual(verdict.extended_code, "HYB-E3001_SHARED_MUT")


if __name__ == "__main__":
    unittest.main()
