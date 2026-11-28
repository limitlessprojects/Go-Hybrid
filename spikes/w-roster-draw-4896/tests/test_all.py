"""One-command test suite.

    python tests/test_all.py

Standard library only, fixed seeds, no network.  The statistical test at the end
is the one that matters for the claim "every roster has probability 1/151": with
a fixed seed it is reproducible, and it is a real chi-square test (df = 150), not
a tolerance eyeball.
"""

from __future__ import annotations

import math
import random
import sys
import unittest
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from hybrid_roster.determinize import PathCounter, SetAutomaton
from hybrid_roster.oracle import (
    brute_product_pairs,
    enumerate_pairs,
    multiplicity_histogram,
    totals,
)
from hybrid_roster.policy import POLICY_A, POLICY_A_SPLIT, Policy, scale_policy
from hybrid_roster.samplers import Determinized, PairsUniform, RosterThenHandoff
from hybrid_roster.stats import chi2_gof, risk_ratio_ci


def oracle_for(policy: Policy) -> dict[str, tuple[int, ...]]:
    return enumerate_pairs(policy.alphabet, policy.horizon, policy.caps,
                           policy.handoff, policy.require_handoff)


class TestIndependentEnumeration(unittest.TestCase):
    def test_issue_numbers(self):
        policy = POLICY_A
        pairs = oracle_for(policy)
        self.assertEqual(totals(pairs), (151, 284))
        self.assertEqual(multiplicity_histogram(pairs), {1: 43, 2: 84, 3: 23, 4: 1})

    def test_three_enumerators_agree(self):
        policy = POLICY_A
        dfs = oracle_for(policy)
        product = brute_product_pairs(policy.alphabet, policy.horizon, policy.caps,
                                      policy.handoff, policy.require_handoff)
        self.assertEqual(dfs, product)
        automaton = SetAutomaton(policy)
        self.assertEqual(automaton.denominator, len(dfs))

    def test_length_sweep_zero_to_nine(self):
        for length in range(10):
            policy = Policy(name=f"n{length}", alphabet="ABC", horizon=length,
                            caps=(("C", 2),))
            automaton = SetAutomaton(policy)
            oracle = oracle_for(policy)
            decoded = [automaton.decode(r) for r in range(automaton.denominator)]
            self.assertEqual(decoded, sorted(oracle), f"n={length}")
            self.assertEqual(automaton.denominator, len(oracle), f"n={length}")

    def test_empty_population_raises(self):
        impossible = Policy(name="impossible", alphabet="A", horizon=3, caps=(("C", 0),))
        automaton = SetAutomaton(impossible)
        self.assertEqual(automaton.denominator, 0)
        with self.assertRaises(ValueError):
            automaton.decode(0)


class TestRankCodec(unittest.TestCase):
    def setUp(self):
        self.policy = POLICY_A
        self.automaton = SetAutomaton(self.policy)
        self.oracle = oracle_for(self.policy)

    def test_every_roster_exactly_once(self):
        decoded = [self.automaton.decode(r) for r in range(self.automaton.denominator)]
        self.assertEqual(len(decoded), 151)
        self.assertEqual(len(set(decoded)), 151)
        self.assertEqual(decoded, sorted(self.oracle))

    def test_rank_of_inverts_decode(self):
        for rank in range(self.automaton.denominator):
            word = self.automaton.decode(rank)
            self.assertEqual(self.automaton.rank_of(word), rank)

    def test_out_of_range_rank(self):
        for bad in (-1, 151, 10**9):
            with self.assertRaises(ValueError):
                self.automaton.decode(bad)

    def test_rank_of_rejects_infeasible_word(self):
        for word in ("AAAAAAAA", "CCCCCCCC", "ACACACAC", "ABABAAAA"):
            try:
                self.automaton.rank_of(word)
            except ValueError:
                continue
            self.fail(f"{word!r} should not be feasible")

    def test_witness_is_legal_and_canonical(self):
        for rank in range(self.automaton.denominator):
            word = self.automaton.decode(rank)
            positions = self.automaton.handoff_positions(word)
            self.assertTrue(positions, word)
            self.assertEqual(self.automaton.witness(word), min(positions))
            self.assertEqual(len(positions), len(self.oracle[word]))
            k = self.automaton.witness(word)
            self.assertEqual((word[k], word[k + 1]), self.policy.handoff)


class TestContract(unittest.TestCase):
    """The equivalent refactor: the handoff branch written twice."""

    def test_visible_language_unchanged(self):
        plain, split = SetAutomaton(POLICY_A), SetAutomaton(POLICY_A_SPLIT)
        self.assertEqual(plain.denominator, split.denominator)
        self.assertEqual([plain.decode(r) for r in range(plain.denominator)],
                         [split.decode(r) for r in range(split.denominator)])

    def test_path_count_changes(self):
        plain, split = PathCounter(POLICY_A), PathCounter(POLICY_A_SPLIT)
        self.assertEqual(plain.denominator, 284)
        self.assertGreater(split.denominator, plain.denominator)
        self.assertEqual(plain.paths_of("ABABACAC"), 2)
        self.assertEqual(split.paths_of("ABABACAC"), 4)

    def test_compact_counting_state_is_equivalent(self):
        for policy in (POLICY_A, scale_policy(8), scale_policy(12)):
            full = SetAutomaton(policy)
            compact = SetAutomaton(policy, compact=True)
            self.assertEqual(full.denominator, compact.denominator)
            self.assertEqual([full.decode(r) for r in range(full.denominator)],
                             [compact.decode(r) for r in range(compact.denominator)])


class TestSamplers(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = POLICY_A
        pairs = oracle_for(cls.policy)
        cls.rosters = sorted(pairs)
        cls.multiplicity = {w: len(b) for w, b in pairs.items()}

    def test_pairs_uniform_matches_multiplicity_model(self):
        sampler = PairsUniform(self.policy)
        rng = random.Random(12345)
        counts = Counter(sampler.draw(rng)[0] for _ in range(60_000))
        observed = {w: counts.get(w, 0) for w in self.rosters}
        mass = sum(self.multiplicity.values())
        stat, df, p = chi2_gof(observed, {w: 60_000 * self.multiplicity[w] / mass
                                          for w in self.rosters})
        self.assertGreater(p, 0.01, f"multiplicity model rejected, chi2={stat:.1f}")
        stat_u, _, p_u = chi2_gof(observed, 1 / 151)
        self.assertLess(p_u, 1e-6, "pair sampling must not look uniform")

    def test_pair_ratio_is_four(self):
        sampler = PairsUniform(self.policy)
        rng = random.Random(999)
        n = 400_000
        counts = Counter(sampler.draw(rng)[0] for _ in range(n))
        rr, lo, hi = risk_ratio_ci(counts["ABABABAB"], n, counts["ABACACBA"], n)
        self.assertTrue(lo < 4.0 < hi, f"ratio CI [{lo:.2f}, {hi:.2f}] misses 4")
        self.assertLess(hi - lo, 1.0, "ratio CI suspiciously wide")

    def test_determinized_roster_law_is_uniform(self):
        """The headline statistical claim: every roster has probability 1/151."""
        sampler = Determinized(self.policy)
        rng = random.Random(20261128)
        counts = Counter(sampler.draw(rng)[0] for _ in range(100_000))
        observed = {w: counts.get(w, 0) for w in self.rosters}
        stat, df, p = chi2_gof(observed, 1 / 151)
        self.assertEqual(df, 150)
        self.assertGreater(p, 0.01, f"uniform law rejected: chi2={stat:.1f}, p={p:.4f}")
        # no roster may drift outside a 99.9% Wilson band
        from hybrid_roster.stats import wilson
        for w in self.rosters:
            lo, hi = wilson(observed[w], 100_000, 3.29)
            self.assertTrue(lo <= 1 / 151 <= hi, w)

    def test_determinized_reveal_is_a_function_of_the_roster(self):
        sampler = Determinized(self.policy)
        rng = random.Random(7)
        first = {}
        for _ in range(20_000):
            word, k = sampler.draw(rng)
            self.assertEqual(first.setdefault(word, k), k)
        self.assertEqual(sampler.reveal_distribution("ABACABAB"), {0: 1.0})

    def test_roster_then_handoff_leaks_reveal_entropy(self):
        sampler = RosterThenHandoff(self.policy, "boundary")
        law = sampler.reveal_distribution("ABACABAB")
        self.assertEqual(law, {0: 1 / 3, 4: 1 / 3, 6: 1 / 3})
        # exact conditional entropy of the reveal, averaged over rosters
        total = 0.0
        for w in self.rosters:
            total += (1 / 151) * (-sum(p * math.log2(p)
                                       for p in sampler.reveal_distribution(w).values()))
        self.assertAlmostEqual(total, 0.81096, places=4)

    def test_reveal_support_is_the_boundary_support(self):
        """Both reveal modes must support exactly the feasible boundaries.

        This is the test that catches the classic off-by-one between "the step
        that consumes position k+1" and "boundary k".
        """
        automaton = SetAutomaton(self.policy)
        for mode in ("boundary", "run"):
            sampler = RosterThenHandoff(self.policy, mode)
            for w in self.rosters[:40]:
                self.assertEqual(sorted(sampler.reveal_distribution(w)),
                                 sorted(automaton.handoff_positions(w)), f"{mode} {w}")

    def test_refactor_shifts_only_the_run_law(self):
        split = RosterThenHandoff(POLICY_A_SPLIT, "run")
        self.assertEqual(split.reveal_distribution("ABACABAB"),
                         {0: 0.5, 4: 0.25, 6: 0.25})
        plain = RosterThenHandoff(POLICY_A_SPLIT, "boundary")
        self.assertEqual(plain.reveal_distribution("ABACABAB"),
                         {0: 1 / 3, 4: 1 / 3, 6: 1 / 3})

    def test_no_private_leak_under_refactor(self):
        plain = Determinized(POLICY_A)
        split = Determinized(POLICY_A_SPLIT)
        rng_a, rng_b = random.Random(4242), random.Random(4242)
        self.assertEqual([plain.draw(rng_a) for _ in range(2_000)],
                         [split.draw(rng_b) for _ in range(2_000)])


if __name__ == "__main__":
    unittest.main(verbosity=2)


