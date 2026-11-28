#!/usr/bin/env python3
"""Unit tests for W-025 (#695): Explicit Desugared MIR (hy-mir) & 1:1 Stateless Emitter."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import mir_verifier as mv


class TestW025HyMirDesugar(unittest.TestCase):
    def test_01_issue_188_const_composite_auto_addr_rejected_via_mir(self):
        res = mv.check_and_emit_mir([
            {"kind": "decl", "name": "s", "is_const": True, "init": "Stats{}"},
            {"kind": "call", "fn": "mutate_ptr", "arg": "s"},
        ], ptr_param_fns={"mutate_ptr"})
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3004 (T22/#188)" in d for d in res.diagnostics))

    def test_02_issue_546_slot_loan_store_back_race_with_task_move_rejected(self):
        res = mv.check_and_emit_mir([
            {"kind": "decl", "name": "m", "is_const": False, "init": "map[string]Item{}"},
            {"kind": "slot_loan", "name": "slot", "map": "m", "key": "k"},
            {"kind": "task_move", "fn": "worker", "target": "m"},
        ], ptr_param_fns=set())
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3104 (#546)" in d for d in res.diagnostics))

    def test_03_post_move_implicit_pointer_auto_addressing_rejected(self):
        res = mv.check_and_emit_mir([
            {"kind": "decl", "name": "buf", "is_const": False, "init": "Buffer{}"},
            {"kind": "task_move", "fn": "send_buf", "target": "buf"},
            {"kind": "call", "fn": "inspect_ptr", "arg": "buf"},
        ], ptr_param_fns={"inspect_ptr"})
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3001" in d for d in res.diagnostics))

    def test_04_clean_mutable_mir_lowering_and_1to1_stateless_emit_bijection(self):
        res = mv.check_and_emit_mir([
            {"kind": "decl", "name": "s", "is_const": False, "init": "Stats{}"},
            {"kind": "call", "fn": "mutate_ptr", "arg": "s"},
            {"kind": "task_move", "fn": "publish", "target": "s"},
        ], ptr_param_fns={"mutate_ptr"})
        self.assertEqual(res.verdict, "ACCEPT")
        self.assertTrue(res.bijection_ok)
        self.assertEqual(res.shadow_tables_used, 0)
        self.assertEqual(len(res.mir), len(res.emitted_go))


if __name__ == "__main__":
    unittest.main()
