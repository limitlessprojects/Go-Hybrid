#!/usr/bin/env python3
"""Unit tests for W-1754 (#1754): Canonical CalleeRef IR + Zero-Transpile hy-vet Mode."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import calleeref_ir as cr


class TestW1754CanonicalCalleeRefIR(unittest.TestCase):
    def setUp(self):
        self.fns = {
            "worker": cr.FnSig(name="worker", type_params=("T",), param_types=("T",)),
            "consume": cr.FnSig(name="consume", type_params=("T",), param_types=("T",), consumes_first=True),
        }

    def test_01_m1_canonical_calleeref_unwraps_parens_generics_and_receivers(self):
        c1 = cr.CalleeRef.resolve("((worker[map[string]int]))")
        self.assertEqual(c1.base_symbol, "worker")
        self.assertEqual(c1.type_args, ("map[string]int",))

        c2 = cr.CalleeRef.resolve("(flate2.compress[[]byte])")
        self.assertEqual(c2.receiver, "flate2")
        self.assertEqual(c2.base_symbol, "compress")
        self.assertEqual(c2.type_args, ("[]byte",))

    def test_02_issue_1752_generic_task_spawn_substitutes_type_args_and_rejects_unmoved(self):
        bindings = {"m": "map[string]int", "n": "int"}
        res_bad = cr.check_calleeref_program(
            self.fns,
            bindings,
            [{"callee": "(worker[map[string]int])", "arg": "m", "is_task": True, "is_move": False}],
        )
        self.assertEqual(res_bad.verdict, "REJECT")
        self.assertTrue(any("HYB-E3002 (#1752)" in d for d in res_bad.diagnostics))

        res_ok = cr.check_calleeref_program(
            self.fns,
            bindings,
            [{"callee": "worker[int]", "arg": "n", "is_task": True, "is_move": False}],
        )
        self.assertEqual(res_ok.verdict, "ACCEPT")

    def test_03_issue_1991_parenthesized_unbound_crate_and_unknown_callee_rejected(self):
        res = cr.check_calleeref_program(
            self.fns,
            {"buf": "[]byte"},
            [{"callee": "((flate2.compress))", "arg": "buf", "is_task": False}],
        )
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3005 (#1991)" in d for d in res.diagnostics))

    def test_04_m2_zero_transpile_hy_and_go_vet_verdict_parity(self):
        hy_verdict = cr.check_calleeref_program(
            self.fns,
            {"b": "own[Buffer]"},
            [
                {"callee": "worker[own[Buffer]]", "arg": "b", "is_task": True, "is_move": True},
                {"callee": "worker[own[Buffer]]", "arg": "b", "is_task": False},
            ],
        )
        go_verdict = cr.check_calleeref_program(
            self.fns,
            {"b": "hy.Unique[Buffer]"},
            [
                {"callee": "worker[hy.Unique[Buffer]]", "arg": "b", "is_task": True, "is_move": True},
                {"callee": "worker[hy.Unique[Buffer]]", "arg": "b", "is_task": False},
            ],
        )
        self.assertEqual(hy_verdict.verdict, "REJECT")
        self.assertEqual(hy_verdict.verdict, go_verdict.verdict)
        self.assertTrue(any("HYB-E3001" in d for d in go_verdict.diagnostics))


if __name__ == "__main__":
    unittest.main()
