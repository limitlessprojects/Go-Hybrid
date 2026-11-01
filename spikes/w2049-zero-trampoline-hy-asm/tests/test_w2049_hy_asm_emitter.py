#!/usr/bin/env python3
"""Unit tests for W-2049 (#2049): Zero-Trampoline Go Assembly Emitter (hy-asm) & Row 20 Gate G2."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import hy_asm_emitter as asm_emit


class TestW2049ZeroTrampolineHyAsm(unittest.TestCase):
    def test_01_amd64_nosplit_leaf_emits_abi_internal_to_sysv_moves_and_beats_5ns(self):
        leaf = asm_emit.RustLeafSpec(symbol="hyrt_hash64", arity=2, dwarf_frame_bytes=64)
        res = asm_emit.emit_hy_asm_stub(leaf, arch="amd64")
        self.assertTrue(res.certified_nosplit)
        self.assertEqual(res.tier, "NOSPLIT_ASM")
        self.assertTrue(res.passes_g2_budget)
        self.assertLess(res.estimated_ns, asm_emit.G2_BUDGET_NS)
        self.assertIn("    MOVQ AX, DI", res.plan9_asm)
        self.assertIn("    MOVQ BX, SI", res.plan9_asm)

    def test_02_arm64_nosplit_leaf_uses_zero_shuffle_register_identity(self):
        leaf = asm_emit.RustLeafSpec(symbol="hyrt_crc32", arity=3, dwarf_frame_bytes=48)
        res = asm_emit.emit_hy_asm_stub(leaf, arch="arm64")
        self.assertTrue(res.certified_nosplit)
        self.assertTrue(res.passes_g2_budget)
        self.assertIn("    BL hyrt_crc32_aapcs64(SB)", res.plan9_asm)

    def test_03_oversized_or_recursive_frame_downgrades_to_deep_guarded_fallback(self):
        deep_leaf = asm_emit.RustLeafSpec(symbol="hyrt_compress_block", arity=2, dwarf_frame_bytes=512)
        res_deep = asm_emit.emit_hy_asm_stub(deep_leaf, arch="amd64")
        self.assertFalse(res_deep.certified_nosplit)
        self.assertEqual(res_deep.tier, "DEEP_GUARDED_FALLBACK")

        rec_leaf = asm_emit.RustLeafSpec(symbol="hyrt_walk_tree", arity=1, dwarf_frame_bytes=64, is_recursive=True)
        res_rec = asm_emit.emit_hy_asm_stub(rec_leaf, arch="amd64")
        self.assertFalse(res_rec.certified_nosplit)
        self.assertEqual(res_rec.tier, "DEEP_GUARDED_FALLBACK")

    def test_04_raw_go_pointer_retention_rejected_fail_closed(self):
        bad_leaf = asm_emit.RustLeafSpec(
            symbol="hyrt_stash_ptr", arity=1, dwarf_frame_bytes=32, retains_go_ptr=True
        )
        res = asm_emit.emit_hy_asm_stub(bad_leaf, arch="amd64")
        self.assertFalse(res.certified_nosplit)
        self.assertEqual(res.tier, "REJECT_UNSAFE_PTR")
        self.assertTrue(any("HYB-E7001" in d for d in res.diagnostics))


if __name__ == "__main__":
    unittest.main()
