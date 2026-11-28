#!/usr/bin/env python3
"""Unit tests for W-020 (#417): Serializable Shared-State Regions via Admitted Transactions."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import stm_region as stm


class TestW020SerializableStmRegions(unittest.TestCase):
    def test_01_write_skew_prevented_by_full_read_set_and_exposed_by_write_only(self):
        # Invariant: a + b >= 1 (initially a=1, b=1). T1 sets a=0 if b==1; T2 sets b=0 if a==1.
        reg_full = stm.TransactionalRegion({"a": 1, "b": 1}, mode="full")
        t1, t2 = reg_full.begin(), reg_full.begin()
        if t1.read("b") == 1:
            t1.write("a", 0)
        if t2.read("a") == 1:
            t2.write("b", 0)
        ok1, _ = reg_full.commit(t1)
        ok2, code2 = reg_full.commit(t2)
        self.assertTrue(ok1)
        self.assertFalse(ok2)
        self.assertEqual(code2, "CONFLICT_READ_SET")
        vals = reg_full.snapshot_values()
        self.assertGreaterEqual(vals["a"] + vals["b"], 1)

        # Negative control 1: write_only validation permits write skew (0, 0)
        reg_bad = stm.TransactionalRegion({"a": 1, "b": 1}, mode="write_only")
        b1, b2 = reg_bad.begin(), reg_bad.begin()
        if b1.read("b") == 1:
            b1.write("a", 0)
        if b2.read("a") == 1:
            b2.write("b", 0)
        self.assertTrue(reg_bad.commit(b1)[0])
        self.assertTrue(reg_bad.commit(b2)[0])
        self.assertEqual(reg_bad.snapshot_values(), {"a": 0, "b": 0})

    def test_02_disjoint_blind_writes_and_stale_base_negative_control(self):
        reg_full = stm.TransactionalRegion({"a": 0, "b": 0}, mode="full")
        t1, t2 = reg_full.begin(), reg_full.begin()
        t1.write("a", 1)
        t2.write("b", 1)
        self.assertTrue(reg_full.commit(t1)[0])
        self.assertTrue(reg_full.commit(t2)[0])
        self.assertEqual(reg_full.snapshot_values(), {"a": 1, "b": 1})

        # Negative control 2: merging into stale snapshot loses T1's committed write
        reg_stale = stm.TransactionalRegion({"a": 0, "b": 0}, mode="stale_base")
        s1, s2 = reg_stale.begin(), reg_stale.begin()
        s1.write("a", 1)
        s2.write("b", 1)
        self.assertTrue(reg_stale.commit(s1)[0])
        self.assertTrue(reg_stale.commit(s2)[0])
        self.assertEqual(reg_stale.snapshot_values(), {"a": 0, "b": 1})

    def test_03_read_your_writes_and_no_ryw_negative_control(self):
        reg_full = stm.TransactionalRegion({"x": 1}, mode="full")
        tx = reg_full.begin()
        tx.write("x", tx.read("x") + 1)
        obs = tx.read("x")
        self.assertTrue(reg_full.commit(tx)[0])
        self.assertEqual(obs, 2)

        reg_no_ryw = stm.TransactionalRegion({"x": 1}, mode="no_ryw")
        tx_bad = reg_no_ryw.begin()
        tx_bad.write("x", tx_bad.read("x") + 1)
        self.assertEqual(tx_bad.read("x"), 1)

    def test_04_lost_update_atomic_transfer_explicit_abort_and_read_only(self):
        reg = stm.TransactionalRegion({"acct_a": 100, "acct_b": 100}, mode="full")
        t1, t2 = reg.begin(), reg.begin()
        t1.write("acct_a", t1.read("acct_a") - 30)
        t1.write("acct_b", t1.read("acct_b") + 30)
        t2.write("acct_a", t2.read("acct_a") - 50)
        t2.abort()
        self.assertFalse(reg.commit(t2)[0])
        self.assertTrue(reg.commit(t1)[0])
        self.assertEqual(reg.snapshot_values(), {"acct_a": 70, "acct_b": 130})

        ro = reg.begin()
        self.assertEqual(ro.read("acct_a") + ro.read("acct_b"), 200)
        self.assertEqual(reg.commit(ro), (True, "COMMITTED_READ_ONLY"))

    def test_05_compile_fail_effect_admission_gates(self):
        res = stm.check_tx_admission([
            {"op": "io", "target": "fmt.println"},
            {"op": "chan_send", "target": "ch"},
            {"op": "task_spawn", "target": "worker"},
            {"op": "call", "target": "unknown_callee"},
        ])
        self.assertFalse(res.admitted)
        self.assertEqual(len(res.diagnostics), 4)
        self.assertTrue(any("HYB-E6001" in d for d in res.diagnostics))
        self.assertTrue(any("HYB-E6002" in d for d in res.diagnostics))
        self.assertTrue(any("HYB-E6003" in d for d in res.diagnostics))
        self.assertTrue(any("HYB-E6004" in d for d in res.diagnostics))


if __name__ == "__main__":
    unittest.main()
