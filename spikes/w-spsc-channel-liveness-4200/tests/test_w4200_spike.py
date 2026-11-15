#!/usr/bin/env python3
import unittest
import sys
import os

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from channel_liveness_verifier import PATTERNS, run_tests

class TestW4200Spike(unittest.TestCase):
    def test_pattern_count(self):
        self.assertEqual(len(PATTERNS), 20)

    def test_runner(self):
        self.assertEqual(run_tests(), 0)

if __name__ == "__main__":
    unittest.main()
