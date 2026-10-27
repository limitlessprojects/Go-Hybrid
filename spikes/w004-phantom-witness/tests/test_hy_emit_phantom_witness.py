#!/usr/bin/env python3
"""Unit tests for W-004 (#174): CPS-Scoped Phantom Witness Emitter."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from tools import hy_emit_phantom_witness as witness_emit


class TestW004PhantomWitnessEmit(unittest.TestCase):
    def test_01_m1_a1b_post_move_use_rejected_via_cps_shadow(self):
        src = """
        fn main() {
            x := make_buf()
            task send(move x)
            println(x)
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3001" in d for d in res.diagnostics))
        self.assertIn("hyCap_root_x := hyMoved{}", res.emitted_go)

    def test_02_m1_a1c_loop_backedge_witness_rejects_loop_move(self):
        src = """
        fn main() {
            x := make_buf()
            for i := range items {
                task send(move x)
            }
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3003 (A1c)" in d for d in res.diagnostics))
        self.assertIn("var _ hyLive[root_x] = hyCap_root_x", res.emitted_go)

    def test_03_m1_issue_27_may_share_alias_invalidates_shared_root(self):
        src = """
        fn main() {
            a := make_map()
            b := a
            task send(move a)
            println(b)
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3001 (A1b/#27)" in d for d in res.diagnostics))

    def test_04_m1_a5_issue_60_non_owned_param_and_closure_capture_move_rejected(self):
        src = """
        fn worker(p Buffer) {
            task send(move p)
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3006 (A5/#60)" in d for d in res.diagnostics))

    def test_05_m1_a2_a3_loan_window_blocks_move_and_mutation(self):
        src = """
        fn main() {
            m := make_map()
            r := &m
            task send(move m)
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertTrue(any("HYB-E3004 (A2/A3)" in d for d in res.diagnostics))

    def test_06_m2_join_point_wall_conditional_move_detected(self):
        src = """
        fn main() {
            x := make_buf()
            if ready {
                task send(move x)
            }
            println(x)
        }
        """
        res = witness_emit.emit_and_verify_witnesses(src)
        self.assertEqual(res.verdict, "REJECT")
        self.assertEqual(res.classification, "CPS_TAIL_DUP")
        self.assertTrue(any("M2 Join-Wall" in d for d in res.diagnostics))

    def test_07_maintainer_review_defer_interplay_and_clean_accept(self):
        src_bad_defer = """
        fn main() {
            x := make_buf()
            defer println(x)
            task send(move x)
        }
        """
        res_bad = witness_emit.emit_and_verify_witnesses(src_bad_defer)
        self.assertEqual(res_bad.verdict, "REJECT")
        self.assertTrue(any("defer-epilogue" in d for d in res_bad.diagnostics))

        src_clean = """
        fn main() {
            x := make_buf()
            println(x)
            task send(move x)
        }
        """
        res_ok = witness_emit.emit_and_verify_witnesses(src_clean)
        self.assertEqual(res_ok.verdict, "ACCEPT")


if __name__ == "__main__":
    unittest.main()
