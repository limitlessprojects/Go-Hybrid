#!/usr/bin/env python3
import os
import sys
import unittest
from itertools import combinations, product

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from recognizer_equivalence import (
    DFA,
    build_number_grammar_fixture,
    build_tag_dfa,
    build_tag_fixture,
    class_vectors_for,
    classify,
    equivalence_classes,
    minimum_test_cover,
)


class TestNumberGrammarFixture(unittest.TestCase):
    """Eight number grammars gated by three independent boolean flags
    (allow_plus, allow_leading_zero, allow_underscore). The issue claims
    three probes give a distinct three-bit answer for all eight."""

    def setUp(self):
        self.dfas, self.labels, self.alphabet = build_number_grammar_fixture()
        self.flags = list(product([False, True], repeat=3))

    def test_each_probe_isolates_exactly_one_flag(self):
        for i, (allow_plus, allow_zero, allow_us) in enumerate(self.flags):
            self.assertEqual(self.dfas[i].run("+0"), allow_plus, self.labels[i])
            self.assertEqual(self.dfas[i].run("00"), allow_zero, self.labels[i])
            self.assertEqual(self.dfas[i].run("1_0"), allow_us, self.labels[i])

    def test_eight_versions_all_pairwise_distinguishable(self):
        classes, distinguishable = equivalence_classes(self.dfas, self.alphabet)
        self.assertEqual(len(classes), 8)
        self.assertEqual(len(distinguishable), len(list(combinations(range(8), 2))))

    def test_minimum_cover_is_exactly_the_three_probes(self):
        result = minimum_test_cover(self.dfas, self.alphabet)
        self.assertTrue(result["exact"])
        self.assertEqual(len(result["witnesses"]), 3)
        self.assertEqual(set(result["witnesses"]), {"+0", "00", "1_0"})

    def test_cover_is_valid_and_two_probes_are_not_enough(self):
        result = minimum_test_cover(self.dfas, self.alphabet)
        witnesses = result["witnesses"]
        n = len(self.dfas)

        vectors = [tuple(d.run(w) for w in witnesses) for d in self.dfas]
        self.assertEqual(len(set(vectors)), n, "the chosen witnesses must separate every revision")

        # 2 bits cannot injectively label 8 revisions, confirming 3 is optimal
        for subset in combinations(witnesses, 2):
            sub_vectors = [tuple(d.run(w) for w in subset) for d in self.dfas]
            self.assertLess(len(set(sub_vectors)), n)

    def test_classify_identifies_each_known_class(self):
        classes, _ = equivalence_classes(self.dfas, self.alphabet)
        cover = minimum_test_cover(self.dfas, self.alphabet)
        vectors = class_vectors_for(self.dfas, classes, cover["witnesses"])
        for cid, members in enumerate(classes):
            observed = [self.dfas[members[0]].run(w) for w in cover["witnesses"]]
            self.assertEqual(classify(vectors, cover["witnesses"], observed), cid)

    def test_classify_is_total_on_all_realizable_answers(self):
        # the three flag-isolating probes are a bijection onto all 8
        # possible 3-bit answers here, so every reachable vector matches
        # a known class; the "unknown" case is exercised below, where the
        # pack does not exhaust the answer space (TestTagFixture).
        classes, _ = equivalence_classes(self.dfas, self.alphabet)
        cover = minimum_test_cover(self.dfas, self.alphabet)
        vectors = class_vectors_for(self.dfas, classes, cover["witnesses"])
        self.assertEqual(len(vectors), 8)


class TestTagFixture(unittest.TestCase):
    """Four tag-accepting revisions, a reject-all baseline, and a sixth
    revision equivalent to the baseline via an extra toggling state. The
    issue claims the minimum cover is four despite five distinct
    behaviors, because every tag is individually required."""

    def setUp(self):
        self.dfas, self.labels, self.alphabet = build_tag_fixture()

    def test_baseline_and_toggler_share_a_class(self):
        classes, distinguishable = equivalence_classes(self.dfas, self.alphabet)
        self.assertEqual(len(classes), 5)
        self.assertNotIn((0, 5), distinguishable)
        baseline_class = next(c for c in classes if 0 in c)
        self.assertIn(5, baseline_class)

    def test_minimum_cover_needs_all_four_tags(self):
        result = minimum_test_cover(self.dfas, self.alphabet)
        self.assertTrue(result["exact"])
        self.assertEqual(len(result["witnesses"]), 4)
        self.assertEqual(set(result["witnesses"]), {"A", "B", "C", "D"})

    def test_each_tag_is_individually_necessary(self):
        for omit in self.alphabet:
            subset = [c for c in self.alphabet if c != omit]
            vectors = [tuple(d.run(w) for w in subset) for d in self.dfas]
            self.assertLess(
                len(set(vectors)), 5,
                "dropping probe %r must stop separating some pair" % omit,
            )

    def test_unmodeled_implementation_can_match_a_class_on_the_pack_alone(self):
        """A match only certifies behavior within the candidate set on the
        chosen pack: an implementation that agrees with tag-A on the pack
        but differs outside it is still reported as tag-A's class."""
        classes, _ = equivalence_classes(self.dfas, self.alphabet)
        cover = minimum_test_cover(self.dfas, self.alphabet)
        vectors = class_vectors_for(self.dfas, classes, cover["witnesses"])

        lookalike_trans = dict(build_tag_dfa("A").transitions)
        lookalike_trans[(1, "Z")] = 2  # differs from V1 only outside the pack's alphabet
        lookalike = DFA(start=0, accept={1, 2}, transitions=lookalike_trans)

        observed = [lookalike.run(w) for w in cover["witnesses"]]
        class_of_v1 = next(cid for cid, members in enumerate(classes) if 1 in members)
        self.assertEqual(classify(vectors, cover["witnesses"], observed), class_of_v1)

        # yet it is not actually equivalent to V1 — a probe outside the
        # pack would tell them apart.
        self.assertNotEqual(lookalike.run("AZ"), self.dfas[1].run("AZ"))

    def test_classify_reports_unknown_for_an_answer_no_class_gives(self):
        # the 4-witness pack only realizes 5 of the 16 possible answer
        # vectors; an implementation accepting both tag A and tag B
        # matches no modeled revision and must be reported as unknown.
        classes, _ = equivalence_classes(self.dfas, self.alphabet)
        cover = minimum_test_cover(self.dfas, self.alphabet)
        vectors = class_vectors_for(self.dfas, classes, cover["witnesses"])
        impossible = tuple(w in ("A", "B") for w in cover["witnesses"])
        self.assertNotIn(impossible, vectors.values())
        self.assertIsNone(classify(vectors, cover["witnesses"], impossible))


class TestCostHonesty(unittest.TestCase):
    """On budget exhaustion, minimum_test_cover must fall back to a
    clearly-marked upper bound rather than silently returning a
    non-optimal cover as if it were exact."""

    def test_budget_exhaustion_is_marked_as_an_upper_bound(self):
        dfas, _, alphabet = build_tag_fixture()
        result = minimum_test_cover(dfas, alphabet, budget=1)
        self.assertFalse(result["exact"])
        self.assertTrue(result.get("upper_bound"))
        self.assertIn("witnesses", result)

    def test_sufficient_budget_recovers_the_exact_minimum(self):
        dfas, _, alphabet = build_tag_fixture()
        result = minimum_test_cover(dfas, alphabet, budget=200_000)
        self.assertTrue(result["exact"])
        self.assertEqual(len(result["witnesses"]), 4)


if __name__ == "__main__":
    unittest.main()
