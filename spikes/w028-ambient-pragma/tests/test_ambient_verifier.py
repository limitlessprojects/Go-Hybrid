"""
Unit & Conformance Tests for W-028 Ambient Pragma Verifier
Validates all 20 canonical patterns across affine moves, goroutine escapes,
sendability propagation, channels, and scope obligations.
"""

from __future__ import annotations
import unittest
from spikes.w028_ambient_pragma.ambient_pragma_verifier import (
    AmbientPragmaVerifier,
    DiagnosticCode,
    TypeDescriptor
)


class TestAmbientPragmaVerifier(unittest.TestCase):
    def setUp(self):
        self.v = AmbientPragmaVerifier()

    def test_p01_happy_path_affine_move_to_goroutine(self):
        # P01: Affine value declared, moved to goroutine arg, no use after move
        ops = [
            {"op": "type_decl", "name": "Resource", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "res", "type": "Resource", "scope": 0},
            {"op": "spawn_goroutine", "args": ["res"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p02_affine_use_after_move_rejected(self):
        # P02: Affine value moved, then read
        ops = [
            {"op": "type_decl", "name": "Handle", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "h", "type": "Handle", "scope": 0},
            {"op": "move", "name": "h"},
            {"op": "use", "name": "h"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p03_affine_double_move_rejected(self):
        # P03: Cannot move affine handle twice
        ops = [
            {"op": "type_decl", "name": "LinearBuf", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "b", "type": "LinearBuf", "scope": 0},
            {"op": "move", "name": "b"},
            {"op": "move", "name": "b"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p04_affine_channel_transfer_moves_ownership(self):
        # P04: Sending affine value to channel consumes it; subsequent use fails
        ops = [
            {"op": "type_decl", "name": "Message", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "msg", "type": "Message", "scope": 0},
            {"op": "send_chan", "val": "msg"},
            {"op": "use", "name": "msg"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p05_non_sendable_pointer_to_goroutine_rejected(self):
        # P05: Passing raw pointer struct to goroutine without sendable pragma triggers E3014
        ops = [
            {"op": "type_decl", "name": "RawState", "is_ptr": True},
            {"op": "var_decl", "name": "s", "type": "RawState", "scope": 0},
            {"op": "spawn_goroutine", "args": ["s"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY)

    def test_p06_sendable_struct_to_goroutine_accepted(self):
        # P06: Struct with //go:hyb sendable pragma accepted across boundary
        ops = [
            {"op": "type_decl", "name": "Config", "pragmas": ["sendable"], "fields": {"id": "int"}},
            {"op": "var_decl", "name": "cfg", "type": "Config", "scope": 0},
            {"op": "spawn_goroutine", "args": ["cfg"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p07_goroutine_reference_capture_rejected(self):
        # P07: Affine value captured by reference in closure without move
        ops = [
            {"op": "type_decl", "name": "Cap", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "c", "type": "Cap", "scope": 0},
            {"op": "spawn_goroutine", "captures": ["c"], "move_captures": False},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.ILLEGAL_BORROW_ESCAPE)

    def test_p08_goroutine_move_capture_accepted(self):
        # P08: Affine value captured with explicit move in closure accepted
        ops = [
            {"op": "type_decl", "name": "Cap", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "c", "type": "Cap", "scope": 0},
            {"op": "spawn_goroutine", "captures": ["c"], "move_captures": True},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p09_unconsumed_affine_obligation_rejected(self):
        # P09: Affine value left unconsumed at scope exit
        ops = [
            {"op": "type_decl", "name": "TxCommit", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "tx", "type": "TxCommit", "scope": 0},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.UNCONSUMED_AFFINE_OBLIGATION)

    def test_p10_nested_struct_transitive_sendability(self):
        # P10: Struct containing non-sendable pointer field rejected on transfer
        ops = [
            {"op": "type_decl", "name": "UnsafeInner", "is_ptr": True},
            {"op": "type_decl", "name": "Outer", "fields": {"inner": "UnsafeInner"}},
            {"op": "var_decl", "name": "out", "type": "Outer", "scope": 0},
            {"op": "send_chan", "val": "out"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY)

    def test_p11_scalar_primitive_transfer_accepted(self):
        # P11: Primitive int scalar transferred across channel
        ops = [
            {"op": "var_decl", "name": "num", "type": "int", "scope": 0},
            {"op": "send_chan", "val": "num"},
            {"op": "use", "name": "num"}, # Scalars are copyable, not moved
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p12_in_place_var_affine_pragma(self):
        # P12: Variable declared with //go:hyb affine pragma directly
        ops = [
            {"op": "var_decl", "name": "tok", "type": "int", "pragmas": ["affine"], "scope": 0},
            {"op": "move", "name": "tok"},
            {"op": "use", "name": "tok"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p13_channel_send_after_move_rejected(self):
        # P13: Channel send attempted on already moved handle
        ops = [
            {"op": "type_decl", "name": "Resource", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "r", "type": "Resource", "scope": 0},
            {"op": "move", "name": "r"},
            {"op": "send_chan", "val": "r"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p14_goroutine_arg_after_move_rejected(self):
        # P14: Goroutine arg passing already moved handle
        ops = [
            {"op": "type_decl", "name": "Resource", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "r", "type": "Resource", "scope": 0},
            {"op": "move", "name": "r"},
            {"op": "spawn_goroutine", "args": ["r"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p15_goroutine_capture_after_move_rejected(self):
        # P15: Goroutine capture of already moved handle
        ops = [
            {"op": "type_decl", "name": "Resource", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "r", "type": "Resource", "scope": 0},
            {"op": "move", "name": "r"},
            {"op": "spawn_goroutine", "captures": ["r"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.USE_AFTER_MOVE)

    def test_p16_multi_field_safe_struct_sendability(self):
        # P16: Composite struct with purely sendable scalars
        ops = [
            {"op": "type_decl", "name": "Point", "fields": {"x": "float64", "y": "float64"}},
            {"op": "var_decl", "name": "pt", "type": "Point", "scope": 0},
            {"op": "send_chan", "val": "pt"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p17_inner_scope_affine_discharge(self):
        # P17: Affine value in inner scope consumed before inner scope exit
        ops = [
            {"op": "type_decl", "name": "LockCap", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "l", "type": "LockCap", "scope": 1},
            {"op": "move", "name": "l"},
            {"op": "scope_exit", "scope": 1},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)

    def test_p18_inner_scope_unconsumed_rejected(self):
        # P18: Affine value in inner scope unconsumed at inner scope exit
        ops = [
            {"op": "type_decl", "name": "LockCap", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "l", "type": "LockCap", "scope": 1},
            {"op": "scope_exit", "scope": 1},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.UNCONSUMED_AFFINE_OBLIGATION)

    def test_p19_non_sendable_captured_in_closure_rejected(self):
        # P19: Raw pointer captured in go func closure
        ops = [
            {"op": "type_decl", "name": "MapWrapper", "is_ptr": True},
            {"op": "var_decl", "name": "m", "type": "MapWrapper", "scope": 0},
            {"op": "spawn_goroutine", "captures": ["m"]},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY)

    def test_p20_pipeline_handoff_three_stages_accepted(self):
        # P20: Pipe stage 1 -> stage 2 -> stage 3 sequential move
        ops = [
            {"op": "type_decl", "name": "Item", "pragmas": ["affine"]},
            {"op": "var_decl", "name": "i", "type": "Item", "scope": 0},
            {"op": "move", "name": "i"},
            {"op": "scope_exit", "scope": 0}
        ]
        diags = self.v.verify_program(ops)
        self.assertEqual(len(diags), 0)


if __name__ == "__main__":
    unittest.main()
