"""
Unit tests for Wildcard Issue #1755 Spike:
Zero-Trampoline SSA Register-ABI Bridge & Linear Place-Capability Tokens.
"""

import unittest
from spikes.zero_trampoline_ssa_place_capabilities.cap_ssa_spike import (
    LinearCapabilityVerifier,
    Perm,
    SliceRegABI,
)


class TestZeroTrampolineAndPlaceCapabilities(unittest.TestCase):
    def test_linear_unique_write_and_double_use_rejection(self) -> None:
        v = LinearCapabilityVerifier()
        c0 = v.mint_entry_unique("c0", "buf")
        self.assertEqual(c0.perm, Perm.UNIQUE)

        # Valid linear write evolves c0 -> c1
        c1 = v.write_with_capability("c0", "c1", "buf")
        self.assertIsNotNone(c1)
        self.assertEqual(v.tokens["c0"].perm, Perm.NONE)
        self.assertEqual(v.tokens["c1"].perm, Perm.UNIQUE)

        # Attempting to reuse consumed token c0 must fail
        stale = v.write_with_capability("c0", "c2", "buf")
        self.assertIsNone(stale)
        self.assertTrue(any("perm=none" in e for e in v.errors))

    def test_disjoint_slice_split_and_rejoin(self) -> None:
        v = LinearCapabilityVerifier()
        v.mint_entry_unique("c0", "audio_frame")

        left, right = v.split_disjoint_slices("c0", "c_left", "c_right", "mid")
        self.assertIsNotNone(left)
        self.assertIsNotNone(right)
        self.assertEqual(v.tokens["c0"].perm, Perm.FROZEN)

        # Writing to parent while frozen is rejected
        bad_parent_write = v.write_with_capability("c0", "c_bad", "audio_frame")
        self.assertIsNone(bad_parent_write)

        # Both disjoint halves can be mutated independently without alias graph explosion
        left2 = v.write_with_capability("c_left", "c_left2", "audio_frame[0:mid]")
        right2 = v.write_with_capability("c_right", "c_right2", "audio_frame[mid:len]")
        self.assertIsNotNone(left2)
        self.assertIsNotNone(right2)

        # Update frozen parent's active children to evolved tokens for rejoin check
        v.tokens["c0"] = v.tokens["c0"].__class__(
            token_id="c0",
            place="audio_frame",
            perm=Perm.FROZEN,
            active_children=("c_left2", "c_right2"),
        )
        restored = v.rejoin_slices("c0", "c_left2", "c_right2", "c_rejoined")
        self.assertIsNotNone(restored)
        self.assertEqual(restored.perm, Perm.UNIQUE)

    def test_ssa_phi_conservation(self) -> None:
        v = LinearCapabilityVerifier()
        v.mint_entry_unique("c_then", "state")
        v.mint_entry_unique("c_else", "state")

        merged = v.verify_phi_merge(["c_then", "c_else"], "c_phi", "state")
        self.assertIsNotNone(merged)
        self.assertEqual(merged.perm, Perm.UNIQUE)
        self.assertEqual(v.tokens["c_then"].perm, Perm.NONE)
        self.assertEqual(v.tokens["c_else"].perm, Perm.NONE)

    def test_zero_trampoline_register_abi_call(self) -> None:
        v = LinearCapabilityVerifier()
        v.mint_entry_unique("c0", "samples")
        abi = SliceRegABI(
            place="samples",
            ptr_reg="RAX",
            len_reg="RBX",
            cap_reg="RCX",
            guaranteed_non_null=True,
            bounds_checked_at_entry=True,
        )
        res = v.verify_zero_trampoline_call(
            slice_abi=abi,
            cap_token_id="c0",
            callee_requires_mut=True,
            stack_headroom_bytes=2048,
            callee_max_frame_bytes=512,
        )
        self.assertTrue(res["direct_reg_abi"])
        self.assertEqual(res["c_abi_trampoline_instructions"], 0)
        self.assertTrue(res["elide_morestack_prologue"])
        self.assertTrue(res["elide_inner_loop_bounds_checks"])


if __name__ == "__main__":
    unittest.main()
