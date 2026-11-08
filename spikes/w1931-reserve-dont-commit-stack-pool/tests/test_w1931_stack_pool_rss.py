"""Unit tests for W-1931 (#1931): Reserve, Don't Commit — Demand-Paged Deep-Stack Pool."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stack_pool_rss import evaluate_deep_stack_pool  # noqa: E402


class TestW1931ReserveDontCommitStackPool(unittest.TestCase):
    def test_01_rss_ratio_under_25_percent_at_1_10_and_100_concurrent_deep_calls(self):
        for slots in (1, 10, 100):
            rep = evaluate_deep_stack_pool(concurrent_slots=slots, recursion_depth=127, bytes_per_frame=384)
            self.assertEqual(rep.status, "OK")
            self.assertFalse(rep.guard_page_faulted)
            self.assertLessEqual(rep.rss_ratio, 0.25)

    def test_02_100_concurrent_depth_127_calls_save_over_95_mib_rss(self):
        rep = evaluate_deep_stack_pool(concurrent_slots=100, recursion_depth=127, bytes_per_frame=384)
        self.assertEqual(rep.eager_rss_bytes, 100 * 1024 * 1024)
        self.assertEqual(rep.demand_paged_rss_bytes, 100 * 12 * 4096)  # 4.6875 MiB vs 100 MiB

    def test_03_runaway_recursion_faults_at_guard_page(self):
        rep = evaluate_deep_stack_pool(concurrent_slots=4, recursion_depth=4000, bytes_per_frame=512)
        self.assertTrue(rep.guard_page_faulted)
        self.assertEqual(rep.status, "W1931_ERR_GUARD_PAGE_FAULT")

    def test_04_falsifies_sub_512_byte_stackless_future_claim_at_depth_127(self):
        rep = evaluate_deep_stack_pool(concurrent_slots=1, recursion_depth=127)
        self.assertGreater(rep.stackless_future_state_bytes, 512)


if __name__ == "__main__":
    unittest.main()
