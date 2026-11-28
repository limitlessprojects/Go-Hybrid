import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from emit_first_gotypes_verifier import (
    GoType,
    GoTypesRawError,
    HySpan,
    check_foreign_const_arg,
    evaluate_wildcard_3941,
    holds_closure,
    instantiate_generic_struct,
    map_gotypes_errors,
    may_share,
)


class TestWildcard3941EmitFirstGoTypes(unittest.TestCase):
    def test_p1_stage1_probes_resolve_without_shape_rs(self) -> None:
        report = evaluate_wildcard_3941()
        self.assertEqual(report.p1_unresolved_stage1, 0)
        self.assertEqual(report.p2_unbuildable_accepted, 0)
        self.assertEqual(report.p2_verdict_flips, 0)

    def test_p2_generic_struct_instantiation_facts_3939_3948(self) -> None:
        t_param = GoType("tparam", "T")
        scalar_int = GoType("scalar", "int")
        map_str_int = GoType("map", "map[string]int", (GoType("scalar", "string"), scalar_int))
        slice_of_maps = GoType("slice", "[]map[string]int", (map_str_int,))
        fn_ty = GoType("func", "func()")

        box_scalar = instantiate_generic_struct("Box", ("T",), (("val", t_param),), (scalar_int,))
        self.assertFalse(may_share(box_scalar))
        self.assertFalse(holds_closure(box_scalar))

        # #3939: Box[map[string]int]
        box_map = instantiate_generic_struct("Box", ("T",), (("val", t_param),), (map_str_int,))
        self.assertTrue(may_share(box_map))
        self.assertFalse(holds_closure(box_map))

        # #3948: Box[[]map[string]int] (compound nested generic argument)
        box_compound = instantiate_generic_struct("Box", ("T",), (("items", t_param),), (slice_of_maps,))
        self.assertTrue(may_share(box_compound))

        # Closure inside generic struct field
        box_fn = instantiate_generic_struct("Box", ("T",), (("cb", t_param),), (fn_ty,))
        self.assertTrue(holds_closure(box_fn))

    def test_p2_cyclic_recursive_struct_terminates(self) -> None:
        # Recursive struct Node { next: *Node } must terminate and report may_share=True
        ptr_node = GoType("pointer", "*Node", (GoType("struct", "Node", (), ()),))
        node = GoType("struct", "Node", (), (("next", ptr_node),))
        self.assertTrue(may_share(node))
        self.assertFalse(holds_closure(node))

    def test_p2_span_map_and_hyb_diagnostic_codes(self) -> None:
        span_map = {
            14: HySpan("prog.hy", 6, 12, "call"),
            22: HySpan("prog.hy", 11, 9, "overflow"),
            29: HySpan("prog.hy", 15, 17, "div_zero"),
        }
        raw = [
            GoTypesRawError(14, "type_mismatch", "cannot use 'x' as int value in argument to fnLocal"),
            GoTypesRawError(22, "const_overflow", "constant 300 overflows uint8"),
            GoTypesRawError(29, "div_by_zero", "invalid operation: division by zero"),
        ]
        mapped = map_gotypes_errors(raw, span_map)
        self.assertEqual([m.code for m in mapped], ["HYB-E1002", "HYB-E2005", "HYB-E2006"])
        self.assertEqual((mapped[0].file, mapped[0].line, mapped[0].col), ("prog.hy", 6, 12))

    def test_p3_foreign_const_slice_falsifier_and_narrowing(self) -> None:
        u8_slice = GoType("slice", "[]byte", (GoType("scalar", "byte"),))
        int_slice = GoType("slice", "[]int", (GoType("scalar", "int"),))

        # Pure signature over-rejects read-only bytes.Equal on const []u8 (Falsifier P3 fires!)
        self.assertEqual(
            check_foreign_const_arg("bytes.Equal", u8_slice, True, use_purity_side_bit=False),
            "HYB-E3006",
        )
        # 1-bit purity side-table admits bytes.Equal while still rejecting sort.Ints
        self.assertIsNone(
            check_foreign_const_arg("bytes.Equal", u8_slice, True, use_purity_side_bit=True)
        )
        self.assertEqual(
            check_foreign_const_arg("sort.Ints", int_slice, True, use_purity_side_bit=True),
            "HYB-E3006",
        )

    def test_p4_latency_and_loc_deletion_budget(self) -> None:
        report = evaluate_wildcard_3941()
        self.assertLess(report.p4_warm_check_ms, 5.0)
        self.assertGreater(report.deleted_rust_loc, 25000)
        self.assertTrue(any("P3 falsifier caught" in n for n in report.notes))


if __name__ == "__main__":
    unittest.main()
