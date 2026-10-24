#!/usr/bin/env python3
"""Unit and live-benchmark tests for Spike W-030 (#1210) by @planners1490."""
from __future__ import annotations

import pathlib
import sys
import unittest

SPIKE_DIR = pathlib.Path(__file__).resolve().parents[1]
if str(SPIKE_DIR) not in sys.path:
    sys.path.insert(0, str(SPIKE_DIR))

import word_atomicity_floor as waf  # noqa: E402


class TestWordAtomicityFloor(unittest.TestCase):
    def test_single_word_primitives_and_channels(self) -> None:
        for ty in ("int", "int64", "uint64", "uintptr", "bool", "chan int", "<-chan string", "sync.mutex"):
            layout = waf.classify_type(ty)
            self.assertEqual(layout.kind, "SINGLE_WORD", ty)
            self.assertEqual(layout.words, 1, ty)
            self.assertEqual(layout.multiword_fields, (), ty)

    def test_multiword_headers_slice_string_any(self) -> None:
        self.assertEqual(waf.classify_type("string").words, 2)
        self.assertEqual(waf.classify_type("string").kind, "MULTI_WORD_HEADER")
        self.assertEqual(waf.classify_type("any").words, 2)
        self.assertEqual(waf.classify_type("any").kind, "MULTI_WORD_HEADER")
        self.assertEqual(waf.classify_type("[]int").words, 3)
        self.assertEqual(waf.classify_type("[]int").kind, "MULTI_WORD_HEADER")

    def test_struct_and_pointer_reachability(self) -> None:
        structs = waf.parse_structs(waf.TORN_HY + "\n" + waf.CONFUSE_HY)
        c_layout = waf.classify_type("*cell", structs)
        self.assertEqual(c_layout.kind, "MULTI_WORD_STRUCT")
        self.assertTrue(len(c_layout.multiword_fields) >= 1)
        plain_layout = waf.classify_type("*plain", structs)
        self.assertEqual(plain_layout.kind, "SINGLE_WORD")

    def test_cyclic_struct_terminates_cleanly(self) -> None:
        src = "struct node { next *node\n count int }"
        structs = waf.parse_structs(src)
        layout = waf.classify_type("*node", structs)
        self.assertEqual(layout.kind, "SINGLE_WORD")

    def test_type_alias_to_multiword_header(self) -> None:
        src = "type buf = []byte\nstruct holder { b buf }"
        structs = waf.parse_structs(src)
        layout = waf.classify_type("*holder", structs)
        self.assertEqual(layout.kind, "MULTI_WORD_STRUCT")
        self.assertEqual(layout.multiword_fields, (("b", "buf"),))

    def test_map_residual_flagged_outside_mutex_category(self) -> None:
        src = """
fn w(m map[string]int) { m["k"] = 1 }
fn main() {
    managed("map share", "other", "T-MAP") {
        var m map[string]int
        task w(m)
    }
}
"""
        crossings = waf.analyze_source(src)
        self.assertEqual(len(crossings), 1)
        self.assertTrue(crossings[0].is_map_residual)

    def test_torn_and_confuse_reproducers_lower_to_atomic_pointer(self) -> None:
        for name, src, expected_field in (
            ("torn.hy", waf.TORN_HY, "s atomic.Pointer[[]int]"),
            ("torn_write.hy", waf.TORN_WRITE_HY, "s atomic.Pointer[[]int]"),
            ("confuse.hy", waf.CONFUSE_HY, "v atomic.Pointer[any]"),
        ):
            lowered, count = waf.lower_to_word_atomic_go(src)
            self.assertEqual(count, 4, name)
            self.assertIn(expected_field, lowered, name)
            self.assertIn(".Store(&", lowered, name)
            self.assertIn(".Load()", lowered, name)

    def test_emitted_go_artifacts_and_box_bench_present(self) -> None:
        res = waf.run_go_verification(run_live_go=True)
        self.assertTrue(res["F0_unlowered_tearing"]["reproduced"])
        self.assertTrue(res["F1_lowered_word_atomic_floor"]["verified_tear_free"])
        self.assertTrue(res["F2_microbenchmark_budget"]["passes_budget"])

    def test_repository_audit_and_zero_corpus_regression(self) -> None:
        rep = waf.audit_repository()
        self.assertEqual(rep["F3_shipped_corpus_zero_cost"]["shipped_emitted_lines_changed"], 0)
        self.assertTrue(rep["F3_shipped_corpus_zero_cost"]["zero_regression"])
        self.assertTrue(all(rep["F4_spec_guarantees_audit"].values()))
        self.assertEqual(waf.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
