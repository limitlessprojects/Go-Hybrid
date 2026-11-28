#!/usr/bin/env python3
"""Tests for the W-4926 callback-boundary checker spike.

Run from the spike folder:
    python3 -m unittest discover -s tests -v
"""

import os
import sys
import unittest
from itertools import product

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from callback_boundary_checker import (  # noqa: E402
    CERTIFIED,
    OUTSIDE_FRAGMENT,
    REJECTED,
    certify,
    make_counter_kernel,
    make_decimator,
    make_flush_kernel,
    partitions_from_mask,
    probe_empty_callbacks,
    probe_exhaustive,
    probe_ramp,
    run_blocks_oracle,
    run_blocks_transducer,
    step_carry,
    step_reset,
    EXPECTED_PROBE,
    EXPECTED_RAMP,
)


class TestCheckerVerdicts(unittest.TestCase):
    def test_carry_phase_is_certified(self):
        verdict = certify(make_decimator("carry"))
        self.assertEqual(verdict.kind, CERTIFIED)
        # Only the three diagonal pairs (q, q) are reachable.
        self.assertEqual(verdict.pairs_explored, 3)
        self.assertIsNone(verdict.witness)

    def test_reset_phase_is_rejected_with_emission_witness(self):
        verdict = certify(make_decimator("reset"))
        self.assertEqual(verdict.kind, REJECTED)
        self.assertIsNotNone(verdict.witness)
        self.assertEqual(verdict.witness["kind"], "emission")

    def test_witness_replays_at_block_level(self):
        kernel = make_decimator("reset")
        witness = certify(kernel).witness
        blocks = witness["blocks"]
        values = witness["input"]
        unbroken = run_blocks_transducer(kernel, 0, [values])
        chunked = run_blocks_transducer(kernel, 0, blocks)
        self.assertEqual(unbroken[1], witness["unbroken_emissions"])
        self.assertEqual(chunked[1], witness["chunked_emissions"])
        self.assertNotEqual(unbroken[1], chunked[1])

    def test_proposal_minimal_example(self):
        # [0] | [1]: one batch emits [0]; restarting emits [0, 1].
        self.assertEqual(step_carry(0, [0, 1]), (2, [0]))
        q, out = 0, []
        for block in ([0], [1]):
            q, emitted = step_reset(q, block)
            out.extend(emitted)
        self.assertEqual(out, [0, 1])


class TestFragmentBoundaries(unittest.TestCase):
    def test_unbounded_state_kernel_is_declined(self):
        verdict = certify(make_counter_kernel())
        self.assertEqual(verdict.kind, OUTSIDE_FRAGMENT)

    def test_flush_kernel_is_declined(self):
        verdict = certify(make_flush_kernel())
        self.assertEqual(verdict.kind, OUTSIDE_FRAGMENT)

    def test_multi_emission_kernel_is_declined(self):
        from callback_boundary_checker import Transducer
        kernel = Transducer(
            name="two-per-sample",
            states=(0,),
            alphabet=(0, 1),
            consume=lambda q, a: (q, (a, a)),
            boundary=lambda q: q,
        )
        self.assertEqual(certify(kernel).kind, OUTSIDE_FRAGMENT)


class TestProbeReproduction(unittest.TestCase):
    def test_exhaustive_numbers_match_the_issue(self):
        self.assertEqual(probe_exhaustive(), EXPECTED_PROBE)

    def test_empty_callback_law(self):
        result = probe_empty_callbacks()
        self.assertTrue(result["carry_preserves"])
        self.assertEqual(result["reset_violating_phases"], [1, 2])

    def test_ramp_counts_match_the_issue(self):
        self.assertEqual(probe_ramp(), EXPECTED_RAMP)


class TestTransducerFaithfulness(unittest.TestCase):
    def test_transducer_fold_matches_block_oracle(self):
        # For both modes, folding the transducer over any partition of any
        # short binary input must reproduce the issue's block-level probe.
        for mode, reset in (("carry", False), ("reset", True)):
            kernel = make_decimator(mode)
            for n in range(0, 7):
                for values in product((0, 1), repeat=n):
                    values = list(values)
                    for mask in range(1 << max(n - 1, 0)):
                        blocks = partitions_from_mask(values, mask) if n else [[]]
                        self.assertEqual(
                            run_blocks_transducer(kernel, 0, blocks),
                            run_blocks_oracle(values, blocks, reset=reset),
                            "%s %s %s" % (mode, values, blocks),
                        )


if __name__ == "__main__":
    unittest.main()
