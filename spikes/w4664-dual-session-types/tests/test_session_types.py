"""Tests for W-4664: Static Dual Session Types for Task Rendezvous."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from session_types import (
    SessionChoreographyVerifier,
    End,
    Send,
    Recv,
    compute_dual,
)


class TestDualSessionTypes(unittest.TestCase):
    def setUp(self):
        self.verifier = SessionChoreographyVerifier()

    def test_duality_computation(self):
        """Dual computation recursively inverts Send/Recv and preserves End."""
        proto = Send("int", Recv("string", End()))
        expected_dual = Recv("int", Send("string", End()))
        self.assertEqual(compute_dual(proto), expected_dual)
        self.assertEqual(compute_dual(expected_dual), proto)

    def test_rendezvous_valid_dual_matches(self):
        """Paired tasks receiving strict dual endpoints pass compile-time rendezvous check."""
        proto_a = Send("int", Recv("string", End()))
        proto_b = compute_dual(proto_a)

        ok, err = self.verifier.verify_rendezvous(expected_type=proto_b, actual_type=proto_b)
        self.assertTrue(ok)
        self.assertIsNone(err)

    def test_falsifier_protocol_inversion_rejected_with_e5002(self):
        """Protocol inversion where both tasks attempt to Send first is rejected with HYB-E5002."""
        # Worker A expects: Send[int, Recv[string, End]] -> Dual is Recv[int, Send[string, End]]
        proto_a = Send("int", Recv("string", End()))
        expected_for_b = compute_dual(proto_a) # Recv[int, Send[string, End]]

        # Erroneous Worker B attempts to Send first: Send[string, Recv[int, End]]
        erroneous_b = Send("string", Recv("int", End()))

        ok, err = self.verifier.verify_rendezvous(expected_type=expected_for_b, actual_type=erroneous_b)
        self.assertFalse(ok)
        self.assertIn("HYB-E5002: Session endpoint duality mismatch", err)
        self.assertIn("expected Recv[int, Send[string, End]]", err)
        self.assertIn("found Send[string, Recv[int, End]]", err)

    def test_falsifier_affine_endpoint_reuse_rejected_with_e5003(self):
        """Reusing a consumed linear session endpoint after a step is rejected with HYB-E5003."""
        proto = Send("int", End())
        ep_a, _ = self.verifier.create_channel("chan_1", proto)

        # Step 1: Send advances endpoint and invalidates ep_a
        next_ep, err = self.verifier.step_send(ep_a, "int")
        self.assertIsNotNone(next_ep)
        self.assertIsNone(err)

        # Step 2: Attempting to reuse old ep_a must fail with HYB-E5003
        reused_ep, err = self.verifier.step_send(ep_a, "int")
        self.assertIsNone(reused_ep)
        self.assertIn("HYB-E5003: Linear session endpoint 'chan_1_a' consumed and invalidated", err)

    def test_emitter_erasure_to_native_go_channels(self):
        """Session protocol types erase to primitive native Go channel types with 0 overhead."""
        proto_int = Send("int", End())
        proto_str = Recv("string", End())

        go_type_int = self.verifier.erase_to_native_go_channel(proto_int)
        go_type_str = self.verifier.erase_to_native_go_channel(proto_str)

        self.assertEqual(go_type_int, "chan int")
        self.assertEqual(go_type_str, "chan string")

    def test_choreography_multi_step_rendezvous(self):
        """Full execution trace of dual endpoints advances state cleanly to End without deadlock."""
        proto = Send("int", Recv("string", End()))
        ep_a, ep_b = self.verifier.create_channel("rendezvous", proto)

        # Task A: Send int
        ep_a1, err = self.verifier.step_send(ep_a, "int")
        self.assertIsNone(err)

        # Task B: Recv int
        ep_b1, err = self.verifier.step_recv(ep_b)
        self.assertIsNone(err)

        # Task B: Send string
        ep_b2, err = self.verifier.step_send(ep_b1, "string")
        self.assertIsNone(err)

        # Task A: Recv string
        ep_a2, err = self.verifier.step_recv(ep_a1)
        self.assertIsNone(err)

        # Both endpoints reached End()
        self.assertEqual(ep_a2.session_type, End())
        self.assertEqual(ep_b2.session_type, End())


if __name__ == "__main__":
    unittest.main()
