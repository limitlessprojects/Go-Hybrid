#!/usr/bin/env python3
"""Unit tests for W-015 (#294): Lexical Split-Join Slice Capabilities via 3-Index Capacity Clipping."""

import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import split_verifier as sv


class TestW015LexicalSplitJoin(unittest.TestCase):
    def test_01_three_index_clip_prevents_append_overwrite_while_two_index_races(self):
        backing = [10, 20, 30, 40]
        base = sv.SliceView(backing, 0, 4, 4)
        left, right = sv.split_three_index(base, 2)
        left_grown = left.append_val(99)
        right.write(0, 42)
        self.assertEqual(right.read(0), 42)
        self.assertEqual(left_grown.read(2), 99)
        self.assertEqual(backing, [10, 20, 42, 40])

        # Negative control: 2-index slice `s[:mid]` clobbers `right[0]` on append
        backing_bad = [10, 20, 30, 40]
        l_bad, r_bad = sv.split_two_index_unsound(sv.SliceView(backing_bad, 0, 4, 4), 2)
        l_bad.append_val(99)
        self.assertEqual(r_bad.read(0), 99)

    def test_02_nested_split_in_split_and_post_block_restoration_accepted(self):
        res = sv.check_split_program([
            {"op": "decl_slice", "name": "s", "elem_type": "int64"},
            {"op": "split", "base": "s", "mid": "4", "left": "left", "right": "right", "body": [
                {"op": "split", "base": "left", "mid": "2", "left": "ll", "right": "lr", "body": [
                    {"op": "move_task", "target": "ll"},
                    {"op": "move_task", "target": "lr"},
                    {"op": "move_task", "target": "right"},
                ]},
            ]},
            {"op": "use", "target": "s"},
        ])
        self.assertEqual(res.verdict, "ACCEPT")
        self.assertIn("left := s[0:4:4]", res.emitted_go)
        self.assertIn("ll := left[0:2:2]", res.emitted_go)

    def test_03_base_access_inside_split_and_fork_use_after_block_rejected(self):
        res_base = sv.check_split_program([
            {"op": "decl_slice", "name": "s"},
            {"op": "split", "base": "s", "mid": "2", "left": "a", "right": "b", "body": [
                {"op": "use", "target": "s"},
            ]},
        ])
        self.assertEqual(res_base.verdict, "REJECT")
        self.assertTrue(any("HYB-E3106" in d for d in res_base.diagnostics))

        res_escape = sv.check_split_program([
            {"op": "decl_slice", "name": "s"},
            {"op": "split", "base": "s", "mid": "2", "left": "a", "right": "b", "body": []},
            {"op": "use", "target": "a"},
        ])
        self.assertEqual(res_escape.verdict, "REJECT")
        self.assertTrue(any("HYB-E1001" in d for d in res_escape.diagnostics))

    def test_04_falsifiers_f1a_non_owned_f1b_const_and_f2_ref_elements_rejected(self):
        res_f1a = sv.check_split_program([
            {"op": "decl_slice", "name": "param_s", "owned": False},
            {"op": "split", "base": "param_s", "mid": "2", "left": "a", "right": "b", "body": []},
        ])
        self.assertEqual(res_f1a.verdict, "REJECT")
        self.assertTrue(any("HYB-E3105 (F1a)" in d for d in res_f1a.diagnostics))

        res_f1b = sv.check_split_program([
            {"op": "decl_slice", "name": "const_s", "is_const": True},
            {"op": "split", "base": "const_s", "mid": "2", "left": "a", "right": "b", "body": []},
        ])
        self.assertEqual(res_f1b.verdict, "REJECT")
        self.assertTrue(any("HYB-E3105 (F1b)" in d for d in res_f1b.diagnostics))

        res_f2 = sv.check_split_program([
            {"op": "decl_slice", "name": "map_slice", "elem_type": "map[string]int"},
            {"op": "split", "base": "map_slice", "mid": "1", "left": "a", "right": "b", "body": []},
        ])
        self.assertEqual(res_f2.verdict, "REJECT")
        self.assertTrue(any("HYB-E3105 (F2)" in d for d in res_f2.diagnostics))


if __name__ == "__main__":
    unittest.main()
