import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import effect_permit_verifier as v


class TestW3269AffineEffectPermits(unittest.TestCase):
    def test_all_gates_pass(self):
        res = v.check_all()
        self.assertTrue(res["verified"], msg=str(res))
        self.assertTrue(res["exhaustive"], msg="bound reached: exploration is not exhaustive")

    def test_gate1_nonvacuous(self):
        g = v.gate_nonvacuous()
        self.assertTrue(g["a_known_completes"])
        self.assertTrue(g["b_permit_returns"])
        self.assertTrue(g["c_independent"])

    def test_gate2_lost_ack_grants_no_second_right(self):
        g = v.gate_lost_ack()
        self.assertTrue(g["emitted_once"])
        self.assertTrue(g["no_second_right"])
        self.assertTrue(g["no_manufactured_right"])

    def test_gate3_conservation(self):
        g = v.gate_conservation()
        self.assertTrue(g["no_duplicate_transfer"])
        self.assertTrue(g["custodian_holds_obligation"])
        self.assertTrue(g["not_erased"])
        self.assertTrue(g["stale_owner_rejected"])

    def test_gate4_counterfactuals_fire(self):
        g = v.gate_counterfactuals()
        self.assertTrue(g["permit_restored_caught"])
        self.assertTrue(g["forged_receipt_caught"])
        self.assertTrue(g["erased_obligation_caught"])

    def test_gate5_plain_retry_is_distinguishable(self):
        g = v.gate_plain_retry_control()
        self.assertEqual(g["plain_retry_emissions"], 3)
        self.assertEqual(g["permit_emissions"], 1)
        self.assertEqual(g["permit_after_three_retries"], 1)

    def test_invariants_hold_on_every_reachable_state(self):
        states, _ = v.explore(20000)
        self.assertGreater(len(states), 0)
        inv = v.check_invariants(states)
        self.assertTrue(inv["ok"], msg=str(inv["violations"]))


if __name__ == "__main__":
    unittest.main()
