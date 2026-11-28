"""Tests for W-4781: Peak Live Buffer Footprint Certification."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from buffer_bound import (
    PrecedenceDAG,
    PeakLiveBufferCertifier,
)


class TestPeakLiveBufferCertifier(unittest.TestCase):
    def test_author_six_event_proposal_example(self):
        """Validates the author's exact 6-event example:
        Chains: A+ -> A- -> C+ -> C- and B+ -> B- -> C-
        Weights: A=8, B=5, C=3.
        Ten legal orders: 4 peak at 8, 6 peak at 13.
        Max peak = 13 attained by witness starting with A+, B+.
        """
        dag = PrecedenceDAG()
        # Chain 1: A+ (8) -> A- (-8) -> C+ (3) -> C- (-3)
        dag.add_event("A+", "A", 8)
        dag.add_event("A-", "A", -8, predecessors=("A+",))
        dag.add_event("C+", "C", 3, predecessors=("A-",))
        # C- has predecessors C+ and B-
        dag.add_event("B+", "B", 5)
        dag.add_event("B-", "B", -5, predecessors=("B+",))
        dag.add_event("C-", "C", -3, predecessors=("C+", "B-"))

        certifier = PeakLiveBufferCertifier(dag)
        res = certifier.certify_peak()

        self.assertEqual(res.total_legal_orders, 10)
        self.assertEqual(res.certified_peak_bytes, 13)
        self.assertEqual(res.peak_distribution.get(8), 4)
        self.assertEqual(res.peak_distribution.get(13), 6)

        # Witness prefix attains 13
        self.assertIn("A+", res.witness_schedule_prefix)
        self.assertIn("B+", res.witness_schedule_prefix)
        self.assertNotIn("A-", res.witness_schedule_prefix)
        self.assertNotIn("B-", res.witness_schedule_prefix)

    def test_independent_concurrent_buffers_sum_peaks(self):
        """Two independent buffers A (10 bytes) and B (20 bytes) can overlap: peak = 30."""
        dag = PrecedenceDAG()
        dag.add_event("A+", "A", 10)
        dag.add_event("A-", "A", -10, predecessors=("A+",))
        dag.add_event("B+", "B", 20)
        dag.add_event("B-", "B", -20, predecessors=("B+",))

        certifier = PeakLiveBufferCertifier(dag)
        res = certifier.certify_peak()

        self.assertEqual(res.certified_peak_bytes, 30)
        self.assertIn(res.peak_distribution[30], [2, 4])

    def test_strictly_serialized_buffers_take_maximum_not_sum(self):
        """Buffer A (10) serialized before Buffer B (20): peak is max(10, 20) = 20, never 30."""
        dag = PrecedenceDAG()
        dag.add_event("A+", "A", 10)
        dag.add_event("A-", "A", -10, predecessors=("A+",))
        dag.add_event("B+", "B", 20, predecessors=("A-",))
        dag.add_event("B-", "B", -20, predecessors=("B+",))

        certifier = PeakLiveBufferCertifier(dag)
        res = certifier.certify_peak()

        self.assertEqual(res.total_legal_orders, 1)
        self.assertEqual(res.certified_peak_bytes, 20)

    def test_zero_capacity_buffer_neutral_impact(self):
        """Zero-sized buffer has 0 weight and does not distort memory calculations."""
        dag = PrecedenceDAG()
        dag.add_event("Z+", "Z", 0)
        dag.add_event("Z-", "Z", 0, predecessors=("Z+",))
        dag.add_event("A+", "A", 15, predecessors=("Z+",))
        dag.add_event("A-", "A", -15, predecessors=("A+",))

        certifier = PeakLiveBufferCertifier(dag)
        res = certifier.certify_peak()

        self.assertEqual(res.certified_peak_bytes, 15)


if __name__ == "__main__":
    unittest.main()
