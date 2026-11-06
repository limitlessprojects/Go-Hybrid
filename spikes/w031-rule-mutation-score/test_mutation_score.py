#!/usr/bin/env python3
"""
Test Suite for W-031 Rule-Mutation Score & Corpus Detection Gate.
Validates:
1. 20 Core diagnostic rules in registry.
2. Mutation operator mechanics (OP_DELETE, OP_INVERT, OP_WIDEN, OP_SWAP).
3. Mutant kill detection when defended by corpus fixtures.
4. Surviving mutant detection and gap reporting when undefended.
5. Exact diagnostic code sensitivity.
6. Overall score computation and GUARANTEES gate pass/fail.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from mutation_score_gate import (
    RuleRegistry,
    RuleDescriptor,
    MutationOp,
    MutantVerdict,
    CorpusFixture,
    MutationScoreEngine
)


class TestMutationScoreGate(unittest.TestCase):
    def setUp(self):
        self.rules = RuleRegistry.get_core_rules()

    def test_rule_registry_has_20_rules(self):
        """Rule registry must contain exactly 20 core diagnostic rules."""
        self.assertEqual(len(self.rules), 20)
        rule_ids = [r.rule_id for r in self.rules]
        self.assertIn("HYB-E3001", rule_ids)
        self.assertIn("HYB-E3003", rule_ids)
        self.assertIn("HYB-E3013", rule_ids)
        self.assertIn("HYB-E4001", rule_ids)

    def test_killed_mutant_when_defended(self):
        """A rule defended by a reject fixture must kill OP_DELETE."""
        rule = RuleDescriptor("HYB-E3003", "Ownership", "Move rejection", 6, True)
        fixtures = [CorpusFixture("corpus/16-move-send-clean.hy", "HYB-E3003", should_reject=True)]
        engine = MutationScoreEngine([rule], fixtures)

        verdict, reason = engine.evaluate_mutant(rule, MutationOp.OP_DELETE)
        self.assertEqual(verdict, MutantVerdict.KILLED)
        self.assertIsNone(reason)

    def test_surviving_mutant_when_undefended(self):
        """A rule with zero protecting fixtures must be flagged as SURVIVED."""
        rule = RuleDescriptor("HYB-E3019", "Unimplemented", "Ghost rule", 99, True)
        fixtures = []  # No fixtures!
        engine = MutationScoreEngine([rule], fixtures)

        verdict, reason = engine.evaluate_mutant(rule, MutationOp.OP_DELETE)
        self.assertEqual(verdict, MutantVerdict.SURVIVED)
        self.assertIn("Zero fixtures protect rule", reason)

    def test_diagnostic_code_swap_killed(self):
        """Swapping diagnostic code (OP_SWAP) must be caught by exact code pins."""
        rule = RuleDescriptor("HYB-E3005", "FailClosed", "Unproven name", 4, True)
        fixtures = [CorpusFixture("corpus/34-select-unproven.hy", "HYB-E3005", should_reject=True)]
        engine = MutationScoreEngine([rule], fixtures)

        verdict, _ = engine.evaluate_mutant(rule, MutationOp.OP_SWAP)
        self.assertEqual(verdict, MutantVerdict.KILLED)

    def test_comprehensive_audit_and_gate_pass(self):
        """With 2 fixtures per rule, mutation score must exceed 90% and pass gate."""
        fixtures = []
        for r in self.rules:
            fixtures.append(CorpusFixture(f"fix_a_{r.rule_id}", r.rule_id, True))
            fixtures.append(CorpusFixture(f"fix_b_{r.rule_id}", r.rule_id, True))

        engine = MutationScoreEngine(self.rules, fixtures)
        audit_res = engine.run_mutation_audit()

        self.assertEqual(audit_res["total_mutants"], 80)
        self.assertEqual(audit_res["killed_mutants"], 80)
        self.assertEqual(audit_res["score_pct"], 100.0)
        self.assertTrue(audit_res["gate_passed"])

    def test_gap_reporting_on_partial_coverage(self):
        """Partial coverage correctly identifies surviving mutants and fails gate."""
        # Only 5 rules covered
        partial_fixtures = [CorpusFixture(f"fix_{self.rules[i].rule_id}", self.rules[i].rule_id, True) for i in range(5)]
        engine = MutationScoreEngine(self.rules, partial_fixtures)
        audit_res = engine.run_mutation_audit()

        self.assertLess(audit_res["score_pct"], 90.0)
        self.assertFalse(audit_res["gate_passed"])
        self.assertGreater(len(audit_res["survivors"]), 0)


if __name__ == "__main__":
    unittest.main()
