#!/usr/bin/env python3
"""
Test Suite for W-030 Verdict-Differential Fuzzing Engine.
Validates:
1. Parse-clean filter correctness.
2. Expected verdict parsing from seed headers.
3. Equivalence-preserving mutations (alpha-renaming, no-op injection, neutral blocks).
4. Verdict flip detection and DDmin minimization to < 40 lines.
5. Deterministic replay from seed.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from verdict_fuzzer import (
    ParseCleanFilter,
    VerdictMutator,
    DeltaDebugger,
    VerdictDifferentialFuzzer,
    SeedProgram
)


class TestVerdictDifferentialFuzzer(unittest.TestCase):
    def setUp(self):
        self.fuzzer = VerdictDifferentialFuzzer(seed=42)

    def test_parse_clean_filter_valid(self):
        """Clean valid programs must pass parse filter."""
        code = """
        fn main() {
            ch := make(chan int)
            ch <- 1
        }
        """
        self.assertTrue(ParseCleanFilter.is_parse_clean(code))

    def test_parse_clean_filter_rejects_unbalanced_braces(self):
        """Unbalanced braces must be rejected by pre-filter."""
        bad_code = """
        fn main() {
            ch := make(chan int)
        """
        self.assertFalse(ParseCleanFilter.is_parse_clean(bad_code))

    def test_extract_expected_verdict_headers(self):
        """Header with // EXPECTED-VERDICT must be parsed correctly."""
        code_accept = "// EXPECTED-VERDICT: ACCEPT\nfn main() {}"
        code_reject = "// EXPECTED-VERDICT: REJECT:HYB-E3003\nfn main() {}"

        self.assertEqual(self.fuzzer.parse_seed_verdict(code_accept), "ACCEPT")
        self.assertEqual(self.fuzzer.parse_seed_verdict(code_reject), "REJECT:HYB-E3003")

    def test_alpha_renaming_preserves_syntax(self):
        """M1 Alpha-renaming mutator must produce parse-clean mutant."""
        code = """
        fn main() {
            counter := 10
            counter := counter + 1
        }
        """
        mutator = VerdictMutator(seed=999)
        mutant = mutator.mutate_m1_alpha_rename(code)
        self.assertTrue(ParseCleanFilter.is_parse_clean(mutant))
        self.assertIn("counter_", mutant)

    def test_identity_injection_preserves_syntax(self):
        """M2 Identity injection mutator must insert _ = 0 and stay parse clean."""
        code = """
        fn main() {
            x := 42
        }
        """
        mutator = VerdictMutator(seed=123)
        mutant = mutator.mutate_m2_identity_injection(code)
        self.assertTrue(ParseCleanFilter.is_parse_clean(mutant))
        self.assertIn("_ = 0", mutant)

    def test_verdict_flip_detection_and_minimization(self):
        """When an oracle flip occurs, DDmin must minimize it to < 40 lines."""
        # 50-line bloated seed
        bloated_lines = ["// EXPECTED-VERDICT: ACCEPT", "fn main() {"]
        for i in range(40):
            bloated_lines.append(f"    val_{i} := {i}")
        bloated_lines.append("    buggy_trigger := 999")
        bloated_lines.append("}")
        source = "\n".join(bloated_lines)

        seed = SeedProgram(source=source, expected_verdict="ACCEPT")

        # Mock checker oracle that simulates a flip whenever 'buggy_trigger' is present
        def mock_checker(code: str) -> str:
            if "buggy_trigger" in code:
                return "REJECT:HYB-E3003"  # Flipped!
            return "ACCEPT"

        hits = self.fuzzer.fuzz_cycle(seed, iterations=10, checker_oracle=mock_checker)
        self.assertEqual(len(hits), 1)

        minimized, expected, actual = hits[0]
        self.assertEqual(expected, "ACCEPT")
        self.assertEqual(actual, "REJECT:HYB-E3003")

        # Must be minimized to under 40 lines (M1/M2 contract)
        line_count = len(minimized.splitlines())
        self.assertLess(line_count, 40, f"Minimized case must be < 40 lines, got {line_count}")
        self.assertIn("buggy_trigger", minimized)

    def test_deterministic_seeded_replay(self):
        """Two fuzzers initialized with the same seed must produce identical mutants."""
        code = """
        fn worker() {
            a := 1
            b := 2
            c := 3
        }
        """
        fuzzer_a = VerdictMutator(seed=777)
        fuzzer_b = VerdictMutator(seed=777)

        mutant_a = fuzzer_a.apply_equivalence_mutation(code)
        mutant_b = fuzzer_b.apply_equivalence_mutation(code)

        self.assertEqual(mutant_a, mutant_b, "Seeded mutations must be strictly deterministic")


if __name__ == "__main__":
    unittest.main()
