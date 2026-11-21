import os
import sys
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '../../tools')))
#!/usr/bin/env python3
import unittest
from unittest.mock import patch, MagicMock
import subprocess
import tempfile
import os

from adversarial_fuzzer import MutationEngine, run_checker, evaluate_mutant, MUTATION_OPERATORS


class TestAdversarialFuzzer(unittest.TestCase):

    def setUp(self):
        self.engine = MutationEngine(seed=123)
        self.base_src = "fn main() {\n    let mut x = 1;\n}\n"

    def test_all_operators_produce_mutations(self):
        for op in MUTATION_OPERATORS:
            mutated, op_used = self.engine.mutate(self.base_src, op)
            self.assertEqual(op, op_used)
            self.assertIsInstance(mutated, str)
            self.assertNotEqual(mutated, "")

    def test_loop_var_mutation_injects_mutation(self):
        mutated, _ = self.engine.mutate(self.base_src, "op_loop_var_mutation")
        self.assertIn("0..1", mutated)

    def test_break_scope_leak_injects_alias(self):
        mutated, _ = self.engine.mutate(self.base_src, "op_break_scope_leak")
        self.assertIn("break;", mutated)

    def test_evaluate_mutant_clean_reject(self):
        mock_proc = subprocess.CompletedProcess(args=[], returncode=1, stdout="", stderr="HYB-E1002")
        with patch("subprocess.run", return_value=mock_proc):
            res = evaluate_mutant("hyb", self.base_src, "op_float_int_mismatch")
            self.assertEqual(res["verdict"], "REJECTED_CLEAN")
            self.assertFalse(res["is_p0"])

    def test_evaluate_mutant_p0_crash(self):
        mock_proc = subprocess.CompletedProcess(args=[], returncode=101, stdout="", stderr="panicked at 'compiler bug'")
        with patch("subprocess.run", return_value=mock_proc):
            res = evaluate_mutant("hyb", self.base_src, "op_break_scope_leak")
            self.assertEqual(res["verdict"], "PANIC_ICE")
            self.assertTrue(res["is_p0"])

    def test_evaluate_mutant_accepted(self):
        mock_proc = subprocess.CompletedProcess(args=[], returncode=0, stdout="", stderr="")
        with patch("subprocess.run", return_value=mock_proc):
            res = evaluate_mutant("hyb", self.base_src, "op_match_arm_conflict")
            self.assertEqual(res["verdict"], "ACCEPTED")
            self.assertFalse(res["is_p0"])


if __name__ == "__main__":
    unittest.main()
