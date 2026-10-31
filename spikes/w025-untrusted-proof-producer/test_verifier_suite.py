#!/usr/bin/env python3
"""
Test Suite for W-025 Untrusted Proof Producer Spike.
Tests all 4 falsifier obligations specified in issue #2176.
"""

import unittest
from certificate_verifier import CertificateVerifier, VerifierError

class TestUntrustedProofProducerFalsifiers(unittest.TestCase):

    def test_f01_supported_unique_transfer_accepted(self):
        """
        Falsifier F-01:
        x := make([]int, 4)
        go worker(x)
        Unique transfer with zero retained aliases must be accepted.
        """
        verifier = CertificateVerifier()
        trace = {
            "allocations": [{"root_id": "r1", "length": 4}],
            "slices": [{"view_id": "v_x", "root_id": "r1", "start": 0, "end": 4}],
            "initial_bindings": [{"task_id": "main", "view_id": "v_x"}],
            "events": [
                {"type": "transfer", "from_task": "main", "to_task": "worker", "view_id": "v_x"},
                {"type": "access", "task": "worker", "view_id": "v_x", "write": True}
            ]
        }
        verdict = verifier.verify_program_trace(trace)
        self.assertEqual(verdict, "CERT_VERIFIED")

    def test_f02_surviving_alias_after_transfer_rejected(self):
        """
        Falsifier F-02:
        x := make([]int, 4)
        alias := x
        go worker(x)  // transfers x, but main retains alias!
        Must reject with ERR_SURVIVING_ALIAS_AFTER_TRANSFER.
        """
        verifier = CertificateVerifier()
        trace = {
            "allocations": [{"root_id": "r1", "length": 4}],
            "slices": [
                {"view_id": "v_x", "root_id": "r1", "start": 0, "end": 4},
                {"view_id": "v_alias", "root_id": "r1", "start": 0, "end": 4}
            ],
            "initial_bindings": [
                {"task_id": "main", "view_id": "v_x"},
                {"task_id": "main", "view_id": "v_alias"}
            ],
            "events": [
                {"type": "transfer", "from_task": "main", "to_task": "worker", "view_id": "v_x"}
            ]
        }
        with self.assertRaises(VerifierError) as cm:
            verifier.verify_program_trace(trace)
        self.assertEqual(cm.exception.code, "ERR_SURVIVING_ALIAS_AFTER_TRANSFER")

    def test_f03_overlapping_concurrent_slice_writes_rejected(self):
        """
        Falsifier F-03:
        s := make([]int, 4)
        s1 := s[:2]   // [0..2)
        s2 := s[1:3]  // [1..3) -> overlaps on index 1!
        go worker(s1)
        s2[0] = 99    // concurrent write to overlapping region
        Must reject with ERR_OVERLAPPING_CONCURRENT_WRITE.
        """
        verifier = CertificateVerifier()
        trace = {
            "allocations": [{"root_id": "r1", "length": 4}],
            "slices": [
                {"view_id": "v_s1", "root_id": "r1", "start": 0, "end": 2},
                {"view_id": "v_s2", "root_id": "r1", "start": 1, "end": 3}
            ],
            "initial_bindings": [
                {"task_id": "worker", "view_id": "v_s1"},
                {"task_id": "main", "view_id": "v_s2"}
            ],
            "events": [
                {
                    "type": "concurrent_access",
                    "task_a": "worker", "view_a": "v_s1", "write_a": True,
                    "task_b": "main", "view_b": "v_s2", "write_b": True
                }
            ]
        }
        with self.assertRaises(VerifierError) as cm:
            verifier.verify_program_trace(trace)
        self.assertEqual(cm.exception.code, "ERR_OVERLAPPING_CONCURRENT_WRITE")

    def test_f04_forged_certificate_bounds_or_root_rejected(self):
        """
        Falsifier F-04:
        Forged certificate asserting out-of-bounds slice range or nonexistent root
        must fail integrity check independently of compiler frontend claims.
        """
        verifier = CertificateVerifier()
        trace = {
            "allocations": [{"root_id": "r1", "length": 4}],
            "slices": [
                {"view_id": "v_forged", "root_id": "r1", "start": 0, "end": 10}  # Out of bounds!
            ],
            "initial_bindings": [],
            "events": []
        }
        with self.assertRaises(VerifierError) as cm:
            verifier.verify_program_trace(trace)
        self.assertEqual(cm.exception.code, "ERR_FORGED_CERTIFICATE_INTEGRITY")

if __name__ == "__main__":
    unittest.main()
