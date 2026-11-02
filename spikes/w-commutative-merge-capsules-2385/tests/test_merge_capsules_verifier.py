import unittest
import sys
import pathlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
import merge_capsules_verifier as v


class TestMergeCapsulesVerifier(unittest.TestCase):
    def test_p1_and_p2_gates(self):
        p1_ok, p2_ok = v.verify_p1_and_p2_pure_python()
        self.assertTrue(p1_ok)
        self.assertTrue(p2_ok)

    def test_falsifiers_fire_as_predicted(self):
        p4a, p4b, p4c = v.verify_falsifiers_p4a_p4b_p4c()
        self.assertTrue(p4a)
        self.assertTrue(p4b)
        self.assertTrue(p4c)

    def test_check_all_with_live_go(self):
        res = v.check_all(live_go=True)
        self.assertTrue(res["verified"], msg=str(res))


if __name__ == "__main__":
    unittest.main()
