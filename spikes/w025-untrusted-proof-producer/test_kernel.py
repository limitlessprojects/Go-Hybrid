#!/usr/bin/env python3
"""
Test Suite for W-025 Untrusted Proof Producer Kernel.
Implements the full falsifier matrix from Issue #2176.
"""

import unittest
from kernel import ProofVerifierKernel, VerifierError


class TestUntrustedProofProducerKernel(unittest.TestCase):

    def test_f01_supported_unique_transfer_accepted(self):
        """
        Falsifier 1:
        x := make([]int, 4)
        go worker(x)
        Unique ownership transfer without retained aliases must pass certification.
        """
        kernel = ProofVerifierKernel()
        proof = {
            "cfg_edges": [{"from": "main:entry", "to": "main:spawn", "type": "spawn"}, {"from": "main:spawn", "to": "worker:run", "type": "step"}],
            "allocations": [{"root_id": "r1", "length": 4, "is_fresh": True}],
            "slices": [{"view_id": "v_x", "root_id": "r1", "start": 0, "end": 4}],
            "initial_bindings": [{"task_id": "main", "view_id": "v_x"}],
            "events": [
                {"type": "transfer", "from_task": "main", "to_task": "worker", "view_id": "v_x"},
                {"type": "access", "task": "worker", "view_id": "v_x", "write": True}
            ],
            "memory_ops": [
                {"addr": "r1", "type": "alloc", "task": "main"},
                {"addr": "v_x", "type": "write", "task": "worker"}
            ]
        }
        actual_cfg = [
            {"from": "main:entry", "to": "main:spawn", "type": "spawn"},
            {"from": "main:spawn", "to": "worker:run", "type": "step"}
        ]
        actual_ops = [
            {"addr": "r1", "type": "alloc", "task": "main"},
            {"addr": "v_x", "type": "write", "task": "worker"}
        ]
        verdict = kernel.verify_proof(proof, actual_cfg, actual_ops)
        self.assertEqual(verdict, "CERT_VERIFIED")

    def test_f02_surviving_alias_writes_after_transfer_rejected(self):
        """
        Falsifier 2:
        x := make([]int, 4)
        alias := x
        go worker(x) // main retains alias!
        Must reject with ERR_SURVIVING_ALIAS_AFTER_TRANSFER.
        """
        kernel = ProofVerifierKernel()
        proof = {
            "cfg_edges": [{"from": "main:1", "to": "main:2"}],
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
            ],
            "memory_ops": []
        }
        actual_cfg = [{"from": "main:1", "to": "main:2"}]
        with self.assertRaises(VerifierError) as cm:
            kernel.verify_proof(proof, actual_cfg, [])
        self.assertEqual(cm.exception.code, "ERR_SURVIVING_ALIAS_AFTER_TRANSFER")

    def test_f03_overlapping_concurrent_slice_writes_rejected(self):
        """
        Falsifier 3:
        s := make([]int, 4)
        s1 := s[:2]   // [0..2)
        s2 := s[1:3]  // [1..3) -> overlaps at index 1
        go worker(s1)
        s2[0] = 99    // concurrent mutation
        Must reject with ERR_OVERLAPPING_CONCURRENT_WRITE.
        """
        kernel = ProofVerifierKernel()
        proof = {
            "cfg_edges": [{"from": "1", "to": "2"}],
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
            ],
            "memory_ops": []
        }
        actual_cfg = [{"from": "1", "to": "2"}]
        with self.assertRaises(VerifierError) as cm:
            kernel.verify_proof(proof, actual_cfg, [])
        self.assertEqual(cm.exception.code, "ERR_OVERLAPPING_CONCURRENT_WRITE")

    def test_f04_forged_certificate_bounds_rejected(self):
        """
        Falsifier 4:
        Forged certificate asserting out-of-bounds slice range or nonexistent root
        must fail integrity check regardless of frontend claims.
        """
        kernel = ProofVerifierKernel()
        proof = {
            "cfg_edges": [{"from": "1", "to": "2"}],
            "allocations": [{"root_id": "r1", "length": 4}],
            "slices": [
                {"view_id": "v_forged", "root_id": "r1", "start": 0, "end": 10}
            ],
            "initial_bindings": [],
            "events": [],
            "memory_ops": []
        }
        actual_cfg = [{"from": "1", "to": "2"}]
        with self.assertRaises(VerifierError) as cm:
            kernel.verify_proof(proof, actual_cfg, [])
        self.assertEqual(cm.exception.code, "ERR_FORGED_CERTIFICATE_INTEGRITY")

    def test_f05_omitted_cfg_edges_rejected(self):
        """
        Falsifier 5:
        Proof omitting actual CFG edges must be rejected with ERR_CFG_COVERAGE_MISMATCH.
        """
        kernel = ProofVerifierKernel()
        proof = {
            "cfg_edges": [{"from": "main:1", "to": "main:2", "type": "step"}],
            "allocations": [],
            "slices": [],
            "initial_bindings": [],
            "events": [],
            "memory_ops": []
        }
        actual_cfg = [
            {"from": "main:1", "to": "main:2", "type": "step"},
            {"from": "main:2", "to": "main:secret_write", "type": "step"}  # Omitted by proof!
        ]
        with self.assertRaises(VerifierError) as cm:
            kernel.verify_proof(proof, actual_cfg, [])
        self.assertEqual(cm.exception.code, "ERR_CFG_COVERAGE_MISMATCH")


if __name__ == "__main__":
    unittest.main()
