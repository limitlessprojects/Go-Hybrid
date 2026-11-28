#!/usr/bin/env python3
"""Gates of wildcard proposal #4644: completion decided by the caller's observation."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
import completion_interval as m  # noqa: E402


class TestSoundness(unittest.TestCase):
    """H1/H2 — completion may not outrun the answer."""

    def test_sound_rule_never_answers_two_things(self):
        cases, failures = m.sweep(m.sound_completion)
        self.assertEqual(cases, 1860)
        self.assertEqual(failures, 0)

    def test_midpoint_rule_is_unsound_often(self):
        cases, failures = m.sweep(m.midpoint_completion)
        self.assertEqual(cases, 1860)
        self.assertGreater(failures, 500, "the naive rule should be the losing one")

    def test_wide_interval_does_not_complete_a_threshold(self):
        count = m.Count([m.Tile(0, 10)])
        self.assertFalse(count.complete(m.Claim(m.ATLEAST, 5)))
        self.assertFalse(count.complete(m.Claim(m.BELOW, 5)))

    def test_answer_on_an_open_claim_raises(self):
        count = m.Count([m.Tile(0, 10)])
        with self.assertRaises(m.ClaimError) as ctx:
            count.answer(m.Claim(m.ATLEAST, 5))
        self.assertEqual(ctx.exception.code, m.E_UNSOUND)
        self.assertEqual(count.answer(m.Claim(m.ATLEAST, 0)), True)


class TestCompletionRules(unittest.TestCase):
    """H3-H5 — each claim kind completes on exactly its own condition."""

    def test_atleast_completes_once_the_lower_bound_reaches_k(self):
        count = m.Count([m.Tile(4, 9), m.Tile(1, 3)])
        self.assertFalse(count.complete(m.Claim(m.ATLEAST, 6)))
        count.refine(1, 2, 2)
        self.assertTrue(count.complete(m.Claim(m.ATLEAST, 6)))

    def test_below_completes_once_the_upper_bound_drops_under_k(self):
        count = m.Count([m.Tile(4, 9)])
        self.assertFalse(count.complete(m.Claim(m.BELOW, 6)))
        count.refine(0, 4, 5)
        self.assertTrue(count.complete(m.Claim(m.BELOW, 6)))
        self.assertEqual(count.answer(m.Claim(m.BELOW, 6)), True)

    def test_exact_never_completes_on_an_interval(self):
        count = m.Count([m.Tile(4, 9)])
        self.assertFalse(count.complete(m.Claim(m.EXACT)))
        count.refine(0, 7, 7)
        self.assertTrue(count.complete(m.Claim(m.EXACT)))
        self.assertEqual(count.point_value(), 7)

    def test_export_and_preview_can_differ_on_the_same_count(self):
        # proposal's own example: preview only needs the 500 warning
        count = m.Count([m.Tile(600, 10 ** 6)])
        self.assertTrue(count.complete(m.Claim(m.ATLEAST, 500)))
        self.assertFalse(count.complete(m.Claim(m.EXACT)))


class TestIntervals(unittest.TestCase):
    """H6/H7 — refinement only tightens; bad tiles are refused."""

    def test_refinement_never_widens(self):
        refinements, widenings = m.monotone_sweep()
        self.assertGreater(refinements, 300)
        self.assertEqual(widenings, 0)

    def test_negative_tile_rejected(self):
        with self.assertRaises(m.ClaimError) as ctx:
            m.Tile(-1, 3)
        self.assertEqual(ctx.exception.code, m.E_NEGATIVE)

    def test_inverted_tile_rejected(self):
        with self.assertRaises(m.ClaimError) as ctx:
            m.Tile(5, 3)
        self.assertEqual(ctx.exception.code, m.E_INVERTED)


if __name__ == "__main__":
    unittest.main()
