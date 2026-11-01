#!/usr/bin/env python3
"""
Unit tests for W-032 Certified Heap-Freedom Task Pipeline Gate.
Validates all 5 rejection rules and valid zero-alloc pipeline patterns.
"""

import unittest
from heapfree_pipeline_gate import HeapFreePipelineGate


class TestHeapFreePipelineGate(unittest.TestCase):
    def setUp(self):
        self.gate = HeapFreePipelineGate()

    def test_reject_task_closure_pointer_capture(self):
        code = """
        @heapfree
        fn dispatchWorker(x: int) {
            val ptr = &x;
            task {
                process(&x);
            };
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4101")
        self.assertIn("task closure captures outer pointer reference", diags[0].message)

    def test_reject_interface_boxing_in_pipeline(self):
        code = """
        @heapfree
        fn runPipeline(payload: Packet) {
            val boxed = any(payload);
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4102")
        self.assertIn("escapes across dynamic interface boxing", diags[0].message)

    def test_reject_dynamic_channel_allocation(self):
        code = """
        @heapfree
        fn setupQueue(dynSize: int) {
            val ch = make(chan Packet, dynSize);
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4103")
        self.assertIn("dynamic channel allocation", diags[0].message)

    def test_reject_escaping_output_reference(self):
        code = """
        @heapfree
        fn createPacket(): &Packet {
            var local: Packet;
            return &local;
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4104")
        self.assertIn("escapes task execution boundary", diags[0].message)

    def test_reject_budget_exceeded(self):
        code = """
        @heapfree(budget=1024)
        fn heavyBuffer() {
            var buf: [2048]byte;
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4105")
        self.assertIn("exceeds declared stack budget", diags[0].message)

    def test_accept_valid_zero_alloc_task_pipeline(self):
        code = """
        @heapfree(budget=4096)
        fn validTaskPipeline(p: Packet) {
            var ring: [64]int;
            val t = _TaskWorker{payload: p};
            t.run();
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, f"Expected 0 diagnostics, got: {diags}")


if __name__ == "__main__":
    unittest.main()
