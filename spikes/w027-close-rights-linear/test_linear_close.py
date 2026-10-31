#!/usr/bin/env python3
"""
Test Suite for W-027 Linear Close-Rights Static Gate.
Covers 10 distinct verification angles and validates M1 milestone expectations:
- 4 Negative rejection tests (compile-fail falsifiers)
- Positive worker pool pattern acceptance
- Aliasing, loops, tasks, borrow semantics, and control-flow boundaries
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from linear_close_gate import LinearCloseGate


class TestLinearCloseRightsGate(unittest.TestCase):
    def setUp(self):
        self.gate = LinearCloseGate()

    # Angle 1: Negative Falsifier 1 - Double Close
    def test_angle_1_reject_double_close(self):
        code = """
        fn main() {
            ch := make(chan int)
            close(ch)
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")
        self.assertIn("double close", diags[0].message)

    # Angle 2: Negative Falsifier 2 - Send After Close
    def test_angle_2_reject_send_after_close(self):
        code = """
        fn main() {
            ch := make(chan int)
            close(ch)
            ch <- 42
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")
        self.assertIn("attempt to send on closed channel", diags[0].message)

    # Angle 3: Aliasing Invariant - Close Via Alias
    def test_angle_3_alias_double_close(self):
        code = """
        fn main() {
            ch1 := make(chan int)
            ch2 := ch1
            close(ch1)
            close(ch2)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 4: Negative Falsifier 3 - Borrowed Channel Close in Callee
    def test_angle_4_reject_close_borrowed(self):
        code = """
        fn worker(c: chan int) {
            close(c)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3016")
        self.assertIn("cannot close borrowed channel", diags[0].message)

    # Angle 5: Negative Falsifier 4 - Concurrent Split Close
    def test_angle_5_reject_split_close_tasks(self):
        code = """
        fn main() {
            ch := make(chan int)
            task {
                close(ch)
            }
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3017")
        self.assertIn("split close hazard", diags[0].message)

    # Angle 6: Loop Invariant - Reject Close Inside Loop
    def test_angle_6_reject_close_inside_loop(self):
        code = """
        fn main() {
            ch := make(chan int)
            for i := 0; i < 5; i++ {
                close(ch)
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3015")
        self.assertIn("cannot call close", diags[0].message)

    # Angle 7: Positive Acceptance - Idiomatic Worker Pool Shutdown
    def test_angle_7_accept_worker_pool(self):
        code = """
        fn main() {
            jobs := make(chan int, 100)
            for i := 0; i < 10; i++ {
                jobs <- i
            }
            close(jobs)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Worker pool pattern must compile with 0 diagnostics")

    # Angle 8: Positive Acceptance - Multiple Channels Managed Independently
    def test_angle_8_independent_channels(self):
        code = """
        fn main() {
            chA := make(chan string)
            chB := make(chan int)
            chA <- "hello"
            close(chA)
            chB <- 100
            close(chB)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0)

    # Angle 9: Send After Close Across Aliases
    def test_angle_9_alias_send_after_close(self):
        code = """
        fn main() {
            ch1 := make(chan int)
            ch2 := ch1
            close(ch2)
            ch1 <- 999
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 10: Unclosed Channel Safe Drop (Affine Drop at Scope Exit)
    def test_angle_10_unclosed_channel_valid_drop(self):
        code = """
        fn main() {
            ch := make(chan int)
            ch <- 1
            // ch drops safely at function exit without explicit close
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0)

    # Angle 11: Security Audit - Multi-Statement Single-Line Bypass (CWE-20)
    def test_angle_11_multistatement_close_catch(self):
        code = """
        fn main() {
            ch := make(chan int)
            close(ch); close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 12: Security Audit - String Literal Injection Masking (CWE-20)
    def test_angle_12_string_literal_masking(self):
        code = """
        fn main() {
            ch := make(chan int)
            msg := "close(ch)"
            ch <- 1
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "String literals must never poison typestate")

    # Angle 13: Security Audit - Defer Close Semantics
    def test_angle_13_defer_close_semantics(self):
        code = """
        fn main() {
            ch := make(chan int)
            defer close(ch)
            ch <- 1
            ch <- 2
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Defer close executes at function exit, not prematurely")

    # Angle 14: Security Audit - Formatting Resilience (Arbitrary Whitespace)
    def test_angle_14_arbitrary_whitespace_close(self):
        code = """
        fn main() {
            ch := make(chan int)
            close  (  ch  )
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 15: Security Audit - Struct & Dotted Field Path Tracking
    def test_angle_15_dotted_field_channel_close(self):
        code = """
        fn main() {
            worker.ch := make(chan int)
            close(worker.ch)
            close(worker.ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 16: Security Audit - Multi-line Raw String Literals
    def test_angle_16_multiline_raw_string(self):
        code = """
        fn main() {
            ch := make(chan int)
            s := `
            close(ch)
            close(ch)
            `
            ch <- 1
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Multi-line raw strings must not trigger false positive")

    # Angle 17: Security Audit - Indexed Channel Expressions (e.g. chans[0])
    def test_angle_17_indexed_channel_double_close(self):
        code = """
        fn main() {
            chans[0] := make(chan int)
            close(chans[0])
            close(chans[0])
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 18: Security Audit - Parenthesized Channel Expressions
    def test_angle_18_parenthesized_expressions(self):
        code = """
        fn main() {
            ch := make(chan int)
            close((ch))
            (ch) <- 1
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 19: Security Audit - Select Statement Send After Close
    def test_angle_19_select_send_after_close(self):
        code = """
        fn main() {
            ch := make(chan int)
            close(ch)
            select {
            case ch <- 42:
            default:
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 20: Security Audit - Chained Send-Receive Expressions (ch1 <- <-ch2)
    def test_angle_20_chained_send_receive(self):
        code = """
        fn main() {
            ch1 := make(chan int)
            ch2 := make(chan int)
            close(ch1)
            ch1 <- <-ch2
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 21: Security Audit - Reading from Closed Channel is Safe and Non-Panicking
    def test_angle_21_receive_after_close_valid(self):
        code = """
        fn main() {
            ch := make(chan int)
            close(ch)
            v := <-ch
            v2, ok := <-ch
            <-ch
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Receives from closed channels must never be rejected")

    # Angle 22: Security Audit - Double Defer Close Prevention
    def test_angle_22_double_defer_close(self):
        code = """
        fn main() {
            ch := make(chan int)
            defer close(ch)
            defer close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3013")

    # Angle 23: Security Audit - Inner Block Loop Escape (CWE-486)
    def test_angle_23_inner_block_loop_escape(self):
        code = """
        fn main() {
            ch := make(chan int)
            for i := 0; i < 5; i++ {
                if i > 2 {
                    // inner block ends with }
                }
                close(ch)
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3015" for d in diags), "Inner block inside loop must not allow loop escape")

    # Angle 24: Security Audit - Inner Block Task Escape (CWE-362)
    def test_angle_24_inner_block_task_escape(self):
        code = """
        fn main() {
            ch := make(chan int)
            task {
                if true {
                }
                close(ch)
            }
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3017" for d in diags), "Inner block inside task must not allow task boundary escape")

    # Angle 25: Security Audit - Unicode Homoglyph Evasion (CWE-1007)
    def test_angle_25_unicode_homoglyph_evasion(self):
        code = """
        fn main() {
            сh := make(chan int)
            close(сh)
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3019" for d in diags), "Homoglyph channel identifier must be detected")

    # Angle 26: Security Audit - Uninitialized Nil Channel Close Panic (CWE-476)
    def test_angle_26_nil_channel_close_panic(self):
        code = """
        fn main() {
            var ch chan int
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3020" for d in diags), "Closing nil channel must be flagged as panic hazard")

    # Angle 27: Security Audit - Tuple Multi-Channel Allocation (CWE-684)
    def test_angle_27_tuple_multichannel_allocation(self):
        code = """
        fn main() {
            ch1, ch2 := make(chan int), make(chan int)
            close(ch1)
            close(ch2)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Tuple channel allocation must register both channels cleanly")

    # Angle 28: Security Audit - Smart Contract Reentrancy Hazard (CWE-841)
    def test_angle_28_smart_contract_reentrancy_hazard(self):
        code = """
        fn settle(target: address) {
            ch := make(chan int)
            invoke_contract(target, "withdraw")
            close(ch)
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3022" for d in diags), "External contract invocation during held channel must be flagged")

    # Angle 29: Security Audit - Range-over-channel Self-Closure (CWE-672)
    def test_angle_29_range_over_channel_self_closure(self):
        code = """
        fn drain() {
            ch := make(chan int)
            for item := range ch {
                close(ch)
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3025" for d in diags), "Range self-closure must be flagged")

    # Angle 30: Security Audit - Asymmetric Branch Conditional Close (CWE-672)
    def test_angle_30_asymmetric_branch_conditional_close(self):
        code = """
        fn handle(flag bool) {
            ch := make(chan int)
            if flag {
                close(ch)
            }
            ch <- 42
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3023" for d in diags), "Sending after conditional close must be flagged")

    # Angle 31: Security Audit - Conditional Defer Close Hazard (CWE-672)
    def test_angle_31_conditional_defer_hazard(self):
        code = """
        fn worker(debug bool) {
            ch := make(chan int)
            if debug {
                defer close(ch)
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3024" for d in diags), "Conditional defer close must be flagged")

    # Angle 32: Security Audit - Nil Channel Send Deadlock Hazard (CWE-835)
    def test_angle_32_nil_channel_send_deadlock(self):
        code = """
        fn sender() {
            var ch chan int
            ch <- 100
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E3020" for d in diags), "Sending to nil channel must be flagged")


if __name__ == "__main__":
    unittest.main()

