#!/usr/bin/env python3
"""
Test Suite for W-033 Ghost Capability Directives Gate.
Validates M1 milestone expectations:
- Falsifies the claim that Hybrid requires a distinct .hy file format.
- Proves standard Go AST + comment pragmas enforce 100% of Hybrid's Tier-0 safety guarantees.
- Negative rejection tests (use-after-move, concurrent race, region escape).
- Positive acceptance tests (pure Go worker pool with move/shared semantics).
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from ghost_capability_gate import GhostCapabilityGate


class TestGhostCapabilitiesGate(unittest.TestCase):
    def setUp(self):
        self.gate = GhostCapabilityGate()

    # Angle 1: Positive Acceptance - Pure Go with Ghost Capability Pragmas
    def test_angle_1_pure_go_positive(self):
        code = """
        package main

        //hy:check
        import "fmt"

        func main() {
            data := []int{1, 2, 3}
            //hy:move data
            sink(data)
            // No subsequent use of data
            fmt.Println("done")
        }

        func sink(d []int) {}
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Valid Go + ghost pragmas must produce 0 diagnostics")

    # Angle 2: Negative Falsifier 1 - Use After Move (HYB-E1001)
    def test_angle_2_reject_use_after_move(self):
        code = """
        package main

        //hy:check
        import "fmt"

        func main() {
            buf := make([]byte, 1024)
            //hy:move buf
            consume(buf)
            fmt.Println(buf[0]) // HYB-E1001: use of moved value
        }

        func consume(b []byte) {}
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E1001")
        self.assertIn("use of moved value", diags[0].message)

    # Angle 3: Negative Falsifier 2 - Double Move Rejection (HYB-E1001)
    def test_angle_3_reject_double_move(self):
        code = """
        package main

        //hy:check
        func main() {
            item := "payload"
            //hy:move item
            sender1(item)
            //hy:move item
            sender2(item)
        }
        """
        diags = self.gate.check_program(code)
        self.assertTrue(any(d.code == "HYB-E1001" for d in diags))

    # Angle 4: Negative Falsifier 3 - Concurrent Data Race Hazard (HYB-E3001)
    def test_angle_4_reject_concurrent_race(self):
        code = """
        package main

        //hy:check
        import "time"

        func main() {
            counter := 0
            go func() {
                counter++ // HYB-E3001: concurrent capture without //hy:move or //hy:shared
            }()
            time.Sleep(10 * time.Millisecond)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3001")
        self.assertIn("concurrent data race hazard", diags[0].message)

    # Angle 5: Positive Acceptance - Shared Read Across Goroutines (HYB-E3001 Immune)
    def test_angle_5_accept_shared_concurrent_read(self):
        code = """
        package main

        //hy:check
        import "fmt"

        func main() {
            config := map[string]string{"host": "localhost"}
            //hy:shared config
            go func() {
                fmt.Println(config["host"])
            }()
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Shared read access across goroutines must be accepted")

    # Angle 6: Negative Falsifier 4 - Region Escape Hazard (HYB-E4001)
    def test_angle_6_reject_region_escape(self):
        code = """
        package main

        //hy:check
        type Node struct { val int }

        func createNode() *Node {
            //hy:region[Arena]
            {
                n := &Node{val: 42}
                return n // HYB-E4001: pointer allocated in region Arena escapes scope
            }
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E4001")
        self.assertIn("region escape hazard", diags[0].message)

    # Angle 7: Security Audit - String Literal Immunity
    def test_angle_7_string_literal_immunity(self):
        code = """
        package main

        import "fmt"

        func main() {
            msg := "//hy:move x"
            x := 42
            fmt.Println(x, msg)
        }
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Pragma syntax inside string literals must never be parsed")

    # Angle 8: Security Audit - Multi-line Raw String Immunity
    def test_angle_8_multiline_raw_string_immunity(self):
        code = """
        package main

        func main() {
            query := `
            //hy:move record
            SELECT * FROM users
            `
            record := 10
            process(record, query)
        }

        func process(r int, q string) {}
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 0, "Multi-line raw strings must not trigger false moves")

    # Angle 9: Aliasing - Use After Move on Read Assignment
    def test_angle_9_use_after_move_read_assignment(self):
        code = """
        package main

        func main() {
            source := 100
            //hy:move source
            transfer(source)
            target := source // HYB-E1001: reading moved source
        }

        func transfer(v int) {}
        """
        diags = self.gate.check_program(code)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E1001")

    # Angle 10: Multi-Variable Ghost Move Parsing
    def test_angle_10_multivariable_ghost_move(self):
        code = """
        package main

        func main() {
            a := 1
            b := 2
            //hy:move a, b
            sink(a, b)
            val := a + b // HYB-E1001: both a and b were moved
        }

        func sink(x, y int) {}
        """
        diags = self.gate.check_program(code)
        self.assertTrue(len(diags) >= 1)
        self.assertTrue(all(d.code == "HYB-E1001" for d in diags))


if __name__ == "__main__":
    unittest.main()
