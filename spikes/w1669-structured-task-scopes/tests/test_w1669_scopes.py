import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from scope_checker import check_structured_scope, run_w1669_adversarial_corpus


class TestW1669StructuredTaskScopes(unittest.TestCase):
    def test_adversarial_corpus_100_percent_separation(self):
        report = run_w1669_adversarial_corpus()
        self.assertTrue(report["all_leaky_rejected"])
        self.assertTrue(report["all_valid_accepted"])
        self.assertEqual(report["leaky_cases_count"], 6)
        self.assertEqual(report["valid_cases_count"], 6)

    def test_unjoined_escape_triggers_hyb_e3501(self):
        res = check_structured_scope("scope {\n  h := task worker()\n}")
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3501")
        self.assertIn("h", res.unjoined_handles)

    def test_double_join_triggers_hyb_e3502(self):
        res = check_structured_scope("scope {\n  h := task worker()\n  join(move h)\n  join(move h)\n}")
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostic_code, "HYB-E3502")

    def test_valid_scope_lowers_to_sync_waitgroup(self):
        res = check_structured_scope("scope {\n  h := task worker()\n  join(move h)\n}")
        self.assertTrue(res.accepted)
        self.assertEqual(res.diagnostic_code, "OK")
        self.assertIn("sync.WaitGroup", res.lowered_go)
        self.assertIn("__hy_wg.Wait()", res.lowered_go)


if __name__ == "__main__":
    unittest.main()
