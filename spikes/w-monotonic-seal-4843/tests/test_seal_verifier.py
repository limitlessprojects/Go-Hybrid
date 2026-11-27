import pathlib
import sys
import unittest

SPIKE_DIR = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SPIKE_DIR))

import seal_verifier as sv


class TestMonotonicSealVerifier(unittest.TestCase):
    def test_d1_corpus19_move_rejects_while_seal_broadcast_accepts(self):
        move_diags = sv.check_corpus19_move_then_sender_read()
        self.assertEqual(move_diags, ["HYB-E3003"], msg=f"corpus #19 must stay REJECT: {move_diags}")

        seal_res = sv.check_d1_seal_broadcast_and_sender_read()
        self.assertEqual(seal_res["diagnostics"], [])
        self.assertIn("[:len(", seal_res["emitted_slice_clip"])

    def test_post_seal_direct_write_and_non_owned_origin_rejected(self):
        self.assertEqual(sv.check_post_seal_direct_write(), ["HYB-E3003"])
        self.assertEqual(sv.check_seal_non_owned_param(), ["HYB-E3004"])

    def test_falsifiers_r1_r2_r3_enforced_by_tier0_rules(self):
        # R1: 3-index slice capacity clipping is mandatory at emission
        self.assertTrue(sv.verify_r1_three_index_clip_invariant())
        # R2: pre-seal interior `may_share` alias write after `seal move` -> HYB-E3003
        self.assertEqual(sv.check_f2_pre_seal_may_share_write(), ["HYB-E3003"])
        # R3: `is_sealable(T)` rejects interior closures, channels, interfaces, managed taint -> HYB-E3015
        self.assertEqual(sv.check_f3_unsealable_shapes(), [
            "HYB-E3015",
            "HYB-E3015",
            "HYB-E3015",
            "HYB-E3015",
        ])

    def test_row9_foreign_callback_readonly_capture_flip(self):
        # Unsealed mutable map captured read-only in foreign callback -> HYB-E3013
        self.assertEqual(sv.check_row9_unsealed_readonly_capture(), ["HYB-E3013"])
        # Sealed map captured in foreign callback -> 0 diagnostics (Guaranteed)
        self.assertEqual(sv.check_row9_sealed_capture(), [])

    def test_hy_fixtures_and_live_go_suite(self):
        report = sv.check_all(live_go=True)
        self.assertTrue(report["verified"], msg=str(report))


if __name__ == "__main__":
    unittest.main()
