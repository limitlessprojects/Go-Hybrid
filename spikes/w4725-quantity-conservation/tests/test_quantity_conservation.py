import unittest
import sys
import os

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), '..')))
from quantity_conservation_model import verify_144144_state_space, run_mutant_anomaly_detection

class TestQuantityConservation(unittest.TestCase):
    def test_state_space_combinations(self):
        count = verify_144144_state_space()
        self.assertEqual(count, 144144, "Must verify exactly 144,144 combinations")

    def test_mutant_detection(self):
        detected = run_mutant_anomaly_detection()
        self.assertTrue(detected, "All mutants must be caught and rejected")

if __name__ == '__main__':
    unittest.main()
