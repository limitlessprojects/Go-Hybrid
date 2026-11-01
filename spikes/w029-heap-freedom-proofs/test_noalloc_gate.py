#!/usr/bin/env python3
"""
Test Suite for W-029 Static Heap-Freedom Proofs (hy:noalloc).
Validates:
- 6 Negative rejection falsifiers (Interface boxing, unbounded append, escaping closures, escaping pointers, foreign calls, recursion).
- 3 Positive real-time kernel acceptances (Audio DSP frames, ring buffers, managed foreign calls).
- Non-noalloc functions remain unconstrained.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from noalloc_gate import NoAllocGate


class TestNoAllocGate(unittest.TestCase):
    def setUp(self):
        self.gate = NoAllocGate()

    # Negative 1: Interface boxing
    def test_reject_interface_boxing(self):
        code = """
        //hy:noalloc
        fn process(val: int) {
            x := any(val)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4001")
        self.assertIn("interface boxing", diags[0].message)

    # Negative 2: Unbounded slice append / dynamic make
    def test_reject_slice_append(self):
        code = """
        //hy:noalloc
        fn appendItem(s: []int, item: int) {
            s = append(s, item)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4002")
        self.assertIn("dynamic slice allocation", diags[0].message)

    # Negative 3: Escaping closure
    def test_reject_escaping_closure(self):
        code = """
        //hy:noalloc
        fn runTask(x: int) {
            cb := fn() { return x }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4003")
        self.assertIn("escaping closure", diags[0].message)

    # Negative 4: Local pointer escape
    def test_reject_local_pointer_escape(self):
        code = """
        //hy:noalloc
        fn getLocal() *int {
            x := 42
            return &x
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4004")
        self.assertIn("address of local stack variable escapes", diags[0].message)

    # Negative 5: Foreign library call without managed
    def test_reject_foreign_call(self):
        code = """
        //hy:noalloc
        fn logEvent(msg: string) {
            fmt.Println(msg)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4006")
        self.assertIn("unproven foreign library call", diags[0].message)

    # Negative 6: Recursive call
    def test_reject_recursion(self):
        code = """
        //hy:noalloc
        fn factorial(n: int) int {
            if n <= 1 { return 1 }
            return n * factorial(n - 1)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4007")
        self.assertIn("recursive call", diags[0].message)

    # Positive 1: Real-time Audio DSP Frame Processing (Fixed Array & Math)
    def test_accept_audio_dsp_kernel(self):
        code = """
        //hy:noalloc
        fn processAudioFrame(gain: float64) {
            var frame [512]float64
            for i := 0; i < 512; i++ {
                frame[i] = frame[i] * gain
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "DSP audio frame buffer must pass with 0 diagnostics")

    # Positive 2: Ring-Buffer Bounded Packet Processor
    def test_accept_ring_buffer_kernel(self):
        code = """
        //hy:noalloc
        fn processRingBuffer(head: int, tail: int) int {
            var ring [1024]byte
            idx := (head + 1) & 1023
            ring[idx] = 0xAA
            return int(ring[idx])
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Ring buffer kernel must pass with 0 diagnostics")

    # Positive 3: Foreign Call with Explicit Managed Permit
    def test_accept_managed_foreign_call(self):
        code = """
        //hy:noalloc
        fn executeRealtime() {
            var buffer [64]byte
            buffer[0] = 1
            managed("emergency diagnostic", "alloc-escape") {
                fmt.Println("debug event")
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Foreign call inside managed permit must be permitted")

    # Positive 4: Unmarked functions are not constrained by noalloc rules
    def test_unmarked_function_unconstrained(self):
        code = """
        fn standardGoFunction() {
            s := make([]int, 10)
            s = append(s, 42)
            fmt.Println(s)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Unmarked function must remain standard Go")


if __name__ == "__main__":
    unittest.main()
