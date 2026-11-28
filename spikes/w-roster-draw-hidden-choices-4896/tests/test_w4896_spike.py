#!/usr/bin/env python3
import os
import sys
import unittest
from collections import Counter

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from roster_draw import (
    START,
    check,
    completions,
    draw,
    oracle_rosters,
    roster_at,
)


class TestRosterDrawSpike(unittest.TestCase):
    """Verifies issue #4896's claim: counting+rank-decoding gives one ticket per
    roster regardless of how many private handoff choices back it."""

    def test_eight_shift_population_matches_issue(self):
        total = completions(START, 8)
        wanted = oracle_rosters(8)
        self.assertEqual(total, 151)
        self.assertEqual(sum(wanted.values()), 284)
        self.assertEqual(
            sorted(Counter(wanted.values()).items()),
            [(1, 43), (2, 84), (3, 23), (4, 1)],
        )

    def test_population_grows_as_documented(self):
        self.assertEqual(
            [completions(START, length) for length in range(10)],
            [0, 0, 1, 4, 11, 27, 58, 102, 151, 204],
        )

    def test_decode_matches_independent_oracle_every_length(self):
        for length in range(10):
            wanted = oracle_rosters(length)
            total = completions(START, length)
            self.assertEqual(total, len(wanted))
            self.assertEqual(
                [roster_at(length, rank) for rank in range(total)],
                sorted(wanted),
            )

    def test_every_rank_decodes_to_a_distinct_valid_roster(self):
        total = completions(START, 8)
        decoded = {roster_at(8, rank) for rank in range(total)}
        self.assertEqual(len(decoded), total)
        for roster in decoded:
            self.assertEqual(roster.count("C") <= 2, True)
            self.assertFalse(any(a == b for a, b in zip(roster, roster[1:])))

    def test_out_of_range_rank_rejected(self):
        total = completions(START, 8)
        with self.assertRaises(ValueError):
            roster_at(8, -1)
        with self.assertRaises(ValueError):
            roster_at(8, total)

    def test_empty_population_rejected(self):
        # Lengths 0 and 1 cannot contain an A-to-B handoff, so no roster
        # is publishable and a draw must fail rather than return a result.
        self.assertEqual(completions(START, 0), 0)
        self.assertEqual(completions(START, 1), 0)
        with self.assertRaises(ValueError):
            draw(0)
        with self.assertRaises(ValueError):
            draw(1)

    def test_negative_length_rejected(self):
        with self.assertRaises(ValueError):
            draw(-1)

    def test_drawn_roster_is_always_in_population(self):
        wanted = oracle_rosters(8)
        for _ in range(200):
            self.assertIn(draw(8), wanted)

    def test_full_self_check_runs_clean(self):
        check()  # raises AssertionError on any mismatch; success is silent


if __name__ == "__main__":
    unittest.main()
