"""Unit tests for W-1935 (#1935): Place-Graph Lowering in Three Provable Stages."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from place_graph_stages import PlaceExpr, PlaceProj, stage1_erase_wrappers, verify_place_stages  # noqa: E402


class TestW1935ThreeStagePlaceGraph(unittest.TestCase):
    def test_01_stage1_paren_erasure_normalizes_wrappers(self):
        raw = PlaceExpr("m", (PlaceProj("Paren"), PlaceProj("Field", "x"), PlaceProj("Paren")), "read")
        clean = stage1_erase_wrappers(raw)
        self.assertEqual(clean.projections, (PlaceProj("Field", "x"),))

    def test_02_stage2_catches_issue_1775_map_slot_subprojection_addr_of(self):
        exprs = [
            PlaceExpr("m", (PlaceProj("Paren"), PlaceProj("MapIndex", "k"), PlaceProj("SliceIndex", "0")), "addr_of")
        ]
        self.assertEqual(verify_place_stages(exprs), (False, "W1935_ERR_1775_UNADDRESSABLE_MAP_PROJECTION"))

    def test_03_stage2_catches_issue_1822_method_receiver_after_move(self):
        exprs = [
            PlaceExpr("buf", (), "move"),
            PlaceExpr("buf", (PlaceProj("Paren"),), "method_mut_recv"),
        ]
        self.assertEqual(verify_place_stages(exprs), (False, "W1935_ERR_1822_USE_OR_BORROW_AFTER_MOVE"))

    def test_04_stage3_disjoint_field_closure_capture_accepted_while_stage2_rejects(self):
        exprs = [
            PlaceExpr("ctx", (PlaceProj("Field", "left"),), "move"),
            PlaceExpr("ctx", (PlaceProj("Field", "right"),), "closure_capture"),
        ]
        self.assertEqual(
            verify_place_stages(exprs, enable_stage3_field_captures=False),
            (False, "W1935_ERR_STAGE2_CONSERVATIVE_WHOLE_ROOT_CAPTURE"),
        )
        self.assertEqual(verify_place_stages(exprs, enable_stage3_field_captures=True), (True, "OK"))


if __name__ == "__main__":
    unittest.main()
