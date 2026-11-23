"""Tests for W-4655: Zero-Allocation Task Graph Certification."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from heapfree_certifier import (
    HeapfreeCertifier,
    TaskNode,
    ValueType,
    ChannelPayload,
    DiagnosticCode,
)


class TestHeapfreeCertifier(unittest.TestCase):
    def setUp(self):
        self.certifier = HeapfreeCertifier()

    def test_certified_heapfree_task_succeeds(self):
        """Standard valid task with static slab allocation and value copy semantics."""
        task = TaskNode(
            task_id="fast_worker",
            is_heapfree=True,
            slab_capacity_bytes=1024,
            local_allocations=[
                ("ring_buf", ValueType("RingBuf[128]", 128, False, True)),
                ("counter", ValueType("Int64", 8, False, True)),
            ],
            closure_captures=[
                ("task_id", ValueType("Int32", 4, False, True)),
            ],
            channel_sends=[
                ChannelPayload("out_ch", ValueType("Int64", 8, False, True), False),
            ],
        )
        res = self.certifier.verify_task(task)
        self.assertTrue(res.passed)
        self.assertEqual(res.code, DiagnosticCode.OK)
        self.assertEqual(res.allocs_per_op, 0)
        self.assertIsNone(res.rejection_reason)

    def test_falsifier_dynamic_heap_allocation_rejected_with_e4001(self):
        """Dynamic slice or pointer allocation inside @heapfree task must be rejected with HYB-E4001."""
        task = TaskNode(
            task_id="bad_alloc_worker",
            is_heapfree=True,
            slab_capacity_bytes=1024,
            local_allocations=[
                ("dynamic_map", ValueType("map[string]int", 64, is_pointer_or_dynamic=True, is_value_copy=False)),
            ],
        )
        res = self.certifier.verify_task(task)
        self.assertFalse(res.passed)
        self.assertEqual(res.code, DiagnosticCode.HYB_E4001)
        self.assertEqual(res.allocs_per_op, 1)
        self.assertIn("HYB-E4001: Slab capacity escape detected", res.rejection_reason)
        self.assertIn("bad_alloc_worker", res.rejection_reason)

    def test_falsifier_closure_capturing_heap_reference_rejected(self):
        """Closure capture that closes over an escaping outer pointer must be rejected with HYB-E4001."""
        task = TaskNode(
            task_id="escaping_closure_worker",
            is_heapfree=True,
            slab_capacity_bytes=512,
            closure_captures=[
                ("shared_ref", ValueType("*SharedState", 8, is_pointer_or_dynamic=True, is_value_copy=False)),
            ],
        )
        res = self.certifier.verify_task(task)
        self.assertFalse(res.passed)
        self.assertEqual(res.code, DiagnosticCode.HYB_E4001)
        self.assertIn("closure captures outer heap reference", res.rejection_reason)

    def test_falsifier_escaping_channel_payload_rejected(self):
        """Channel send containing escaping dynamic payload must be rejected with HYB-E4001."""
        task = TaskNode(
            task_id="channel_escape_worker",
            is_heapfree=True,
            slab_capacity_bytes=1024,
            channel_sends=[
                ChannelPayload("stream_ch", ValueType("[]byte", 24, is_pointer_or_dynamic=True, is_value_copy=False), escapes_lexical_scope=True),
            ],
        )
        res = self.certifier.verify_task(task)
        self.assertFalse(res.passed)
        self.assertEqual(res.code, DiagnosticCode.HYB_E4001)
        self.assertIn("escapes lexical epicycle", res.rejection_reason)

    def test_adversarial_fanout_fanin_benchmark_zero_allocs(self):
        """100k messages/sec across 16 worker tasks achieves 0 allocs/op and 0 STW sweeps."""
        stats = self.certifier.benchmark_pipeline(num_workers=16, num_messages=100000)
        self.assertEqual(stats["total_messages"], 100000)
        self.assertEqual(stats["workers"], 16)
        self.assertTrue(stats["certified_heapfree"])
        self.assertEqual(stats["allocs_per_op"], 0)
        self.assertEqual(stats["heap_escapes"], 0)
        self.assertEqual(stats["gc_stw_sweeps"], 0)


if __name__ == "__main__":
    unittest.main()
