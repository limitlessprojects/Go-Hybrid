"""Unit tests for W-1942 (#1942): Fuel-Metered Compute Capsules with a Closed Call Graph."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fuel_capsule import CapsuleStep, run_fuel_capsule  # noqa: E402


class TestW1942FuelMeteredCapsule(unittest.TestCase):
    def test_01_bounded_scalar_and_byte_vec_computation_succeeds(self):
        steps = [
            CapsuleStep("scalar_add", val=10),
            CapsuleStep("call_frame"),
            CapsuleStep("byte_scan", byte_len=16),
            CapsuleStep("return_frame"),
        ]
        res = run_fuel_capsule(steps, initial_fuel=25)
        self.assertTrue(res.admitted)
        self.assertEqual(res.status, "OK")
        self.assertEqual(res.value, 26)
        self.assertEqual(res.fuel_remaining, 25 - (1 + 1 + 16 + 1))

    def test_02_pre_transition_debit_exits_with_budget_exceeded_before_work(self):
        steps = [
            CapsuleStep("scalar_add", val=5),
            CapsuleStep("byte_scan", byte_len=32),
        ]
        res = run_fuel_capsule(steps, initial_fuel=10)
        self.assertTrue(res.admitted)
        self.assertEqual(res.status, "BudgetExceeded")
        self.assertIsNone(res.value)
        self.assertEqual(res.fuel_remaining, 9)
        self.assertEqual(res.transitions_executed, 1)

    def test_03_frame_depth_overflow_exits_cleanly(self):
        steps = [CapsuleStep("call_frame") for _ in range(5)]
        res = run_fuel_capsule(steps, initial_fuel=100, max_frame_depth=3)
        self.assertEqual(res.status, "FrameDepthExceeded")

    def test_04_forbidden_ambient_go_task_ffi_managed_and_fuel_reset_rejected(self):
        for eff, code in [
            ("ambient_go_call", "W1942_ERR_AMBIENT_GO_CALL"),
            ("callback_indirect", "W1942_ERR_INDIRECT_CALLBACK"),
            ("task_or_chan", "W1942_ERR_TASK_OR_CHAN_OP"),
            ("rust_ffi_bridge", "W1942_ERR_RUST_BRIDGE_CALL"),
            ("managed_hatch", "W1942_ERR_MANAGED_FORBIDDEN"),
            ("catch_fuel_reset", "W1942_ERR_FUEL_RESET_FORBIDDEN"),
        ]:
            res = run_fuel_capsule([CapsuleStep(eff)], initial_fuel=50)
            self.assertFalse(res.admitted)
            self.assertEqual(res.status, code)

    def test_05_oversized_byte_vector_rejected_at_admission(self):
        res = run_fuel_capsule([CapsuleStep("byte_scan", byte_len=8192)], initial_fuel=10000, max_byte_vec=4096)
        self.assertFalse(res.admitted)
        self.assertEqual(res.status, "W1942_ERR_UNBOUNDED_BYTE_VEC")


if __name__ == "__main__":
    unittest.main()
