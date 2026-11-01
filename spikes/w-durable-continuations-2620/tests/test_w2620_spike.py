import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import durable_continuations_verifier as v


class TestW2620DurableContinuationsSpike(unittest.TestCase):
    def test_admission_and_recovery_gates(self):
        p1_ok, p2_ok, negs_ok, metrics = v.verify_pure_python_gates()
        self.assertTrue(p1_ok)
        self.assertTrue(p2_ok)
        self.assertTrue(negs_ok)
        self.assertGreater(metrics["saved_steps_75pct"], 0)
        self.assertLess(metrics["snapshot_bytes"], 1024)

    def test_unbounded_channel_and_dynamic_spawn_rejected(self):
        src = """
        fn bad() {
            ch := make(chan int)
            for i in 0..4 {
                task worker(i, ch)
            }
        }
        """
        codes = {d["code"] for d in v.check_durable_region_admission(src)}
        self.assertIn("HYB-E3620-UNBOUNDED-CHAN", codes)
        self.assertIn("HYB-E3620-DYNAMIC-SPAWN", codes)

    def test_check_all_with_live_go(self):
        res = v.check_all(live_go=True)
        self.assertTrue(res["verified"], msg=str(res))


if __name__ == "__main__":
    unittest.main()
