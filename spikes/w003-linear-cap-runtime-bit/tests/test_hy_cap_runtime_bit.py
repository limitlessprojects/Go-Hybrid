#!/usr/bin/env python3
"""Unit tests for W-003 (#193): Linear Capability Surface + 1-Bit Runtime Header Guard."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from tools import hy_cap_runtime_bit as cap_bit


class TestW003LinearCapRuntimeBit(unittest.TestCase):
    def test_01_linear_own_rejects_unannotated_copy_and_post_move_use(self):
        rep_copy = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "m", "cap": "own[map[string]int]"},
            {"op": "rebind", "src": "m", "dst": "m2", "mode": "copy"},
        ])
        self.assertEqual(rep_copy.verdict, "REJECT")
        self.assertTrue(any("HYB-E3002" in e for e in rep_copy.static_errors))

        rep_uam = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "m", "cap": "own[map[string]int]"},
            {"op": "move", "target": "m"},
            {"op": "read", "target": "m"},
        ])
        self.assertEqual(rep_uam.verdict, "REJECT")
        self.assertTrue(any("HYB-E3001" in e for e in rep_uam.static_errors))
        self.assertTrue(any("HYB-R3001" in t for t in rep_uam.runtime_traps))

    def test_02_corpus_rows_21_to_30_loop_move_and_view_boundary_rejected(self):
        rep_loop = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "buf", "cap": "own[[]byte]"},
            {"op": "send", "target": "buf"},
        ], in_loop=True)
        self.assertEqual(rep_loop.verdict, "REJECT")
        self.assertTrue(any("HYB-E3003" in e for e in rep_loop.static_errors))

        rep_view = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "m", "cap": "own[map[string]int]"},
            {"op": "rebind", "src": "m", "dst": "v", "mode": "view"},
            {"op": "task_capture", "target": "v"},
        ])
        self.assertEqual(rep_view.verdict, "REJECT")
        self.assertTrue(any("HYB-E3004" in e for e in rep_view.static_errors))

    def test_03_receipts_110_111_123_share_peer_freeze_and_delete_rejected(self):
        rep = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "m1", "cap": "share[map[string]int]", "header_id": "hdr_map"},
            {"op": "rebind", "src": "m1", "dst": "m2", "mode": "copy"},
            {"op": "send", "target": "m1"},
            {"op": "delete", "target": "m2"},
        ])
        self.assertEqual(rep.verdict, "REJECT")
        self.assertTrue(any("HYB-E3006" in e for e in rep.static_errors))
        self.assertTrue(any("HYB-R3001" in t for t in rep.runtime_traps))

    def test_04_triage_q1_ambient_go_aliasing_caught_by_1bit_header_guard(self):
        rep = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "s", "cap": "own[[]int]", "header_id": "hdr_slice"},
            {"op": "rebind", "src": "s", "dst": "ambient_copy", "mode": "ambient_alias"},
            {"op": "move", "target": "s"},
            {"op": "write", "target": "ambient_copy"},
        ])
        self.assertEqual(rep.verdict, "REJECT")
        self.assertEqual(rep.headers["hdr_slice"].padding_bytes_added, 0)
        self.assertTrue(any("HYB-R3001" in t for t in rep.runtime_traps))

    def test_05_triage_q2_rust_bridge_into_raw_and_from_raw_bit_lifecycle(self):
        rep_trap = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "buf", "cap": "own[[]byte]", "header_id": "hdr_ffi"},
            {"op": "ffi_into_rust", "target": "buf"},
            {"op": "read", "target": "buf"},
        ])
        self.assertEqual(rep_trap.verdict, "REJECT")
        self.assertTrue(any("HYB-R3002" in t for t in rep_trap.runtime_traps))

        rep_reclaim = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "buf", "cap": "own[[]byte]", "header_id": "hdr_ffi2"},
            {"op": "ffi_into_rust", "target": "buf"},
            {"op": "ffi_from_rust", "target": "buf"},
            {"op": "write", "target": "buf"},
        ])
        self.assertEqual(rep_reclaim.verdict, "ACCEPT")

    def test_06_triage_q3_w011_go_surface_erasure_compatibility(self):
        rep = cap_bit.verify_capability_ops([
            {"op": "decl", "name": "pkg_state", "cap": "hyb.Own[*State]"},
            {"op": "write", "target": "pkg_state"},
            {"op": "send", "target": "pkg_state"},
        ])
        self.assertEqual(rep.verdict, "ACCEPT")


if __name__ == "__main__":
    unittest.main()
