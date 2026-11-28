import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ssa_lowering import (
    SSALoweringEngine, SSAOp, FrameSlot, GCStackMap, SSAVersionAdapter, SSAFunction,
    HyLitInt, HyLitFloat, HyLitStr, HyLitBool, HyIdent, HyBinary, HyCall,
    HyField, HyChanRecv, HyChanSend, HyAssign, HyIf, HyWhile, HyReturn,
    HyDefer, HyBreak, HyContinue, HySelect, HyStructAlloc
)


class TestSSALoweringEngine(unittest.TestCase):
    def setUp(self):
        self.engine = SSALoweringEngine()

    def test_01_straight_line_arithmetic_ops(self):
        stmts = [
            HyAssign("x", HyLitInt(10)),
            HyAssign("y", HyLitInt(20)),
            HyAssign("z", HyBinary("+", HyIdent("x"), HyIdent("y"))),
            HyReturn(HyIdent("z")),
        ]
        fn = self.engine.lower_function("calc", stmts)
        self.assertEqual(len(fn.blocks), 1)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertEqual(ops, [SSAOp.CONST_INT, SSAOp.CONST_INT, SSAOp.ADD, SSAOp.RETURN])

    def test_02_primitive_types_typed_correctly(self):
        stmts = [
            HyAssign("fl", HyLitFloat(3.14159)),
            HyAssign("b", HyLitBool(True)),
            HyAssign("s", HyLitStr("hybrid_engine")),
        ]
        fn = self.engine.lower_function("types", stmts)
        values = fn.blocks[0].values
        self.assertEqual(values[0].val_type, "float64")
        self.assertEqual(values[1].val_type, "bool")
        self.assertEqual(values[2].val_type, "string")

    def test_03_owned_binding_receives_stack_certificate(self):
        stmts = [
            HyAssign("buf", HyLitInt(1024), is_owned=True, escapes=False),
            HyReturn(HyIdent("buf")),
        ]
        fn = self.engine.lower_function("stack_alloc", stmts)
        local_addrs = [v for v in fn.blocks[0].values if v.op == SSAOp.LOCAL_ADDR]
        self.assertEqual(len(local_addrs), 1)
        self.assertTrue(local_addrs[0].stack_certified)
        self.assertEqual(self.engine.stack_allocations, 1)
        self.assertEqual(self.engine.heap_allocations, 0)

    def test_04_mutation_falsifier_escaping_owned_place_revokes_certificate(self):
        stmts = [
            HyAssign("leaked_buf", HyLitInt(4096), is_owned=True, escapes=True),
        ]
        _ = self.engine.lower_function("falsifier_escape", stmts)
        self.assertEqual(self.engine.revoked_certificates, 1)
        self.assertEqual(self.engine.heap_allocations, 1)
        self.assertEqual(self.engine.stack_allocations, 0)

    def test_05_if_else_branching_inserts_phi_node_with_clean_cfgs(self):
        stmts = [
            HyIf(
                cond=HyBinary("==", HyLitInt(1), HyLitInt(1)),
                then_stmts=[HyAssign("res", HyLitInt(100))],
                else_stmts=[HyAssign("res", HyLitInt(200))],
            ),
            HyReturn(HyIdent("res")),
        ]
        fn = self.engine.lower_function("branch_phi", stmts)
        self.assertEqual(len(fn.blocks), 4)
        merge_block = fn.blocks[-1]
        phis = [v for v in merge_block.values if v.op == SSAOp.PHI]
        self.assertEqual(len(phis), 1)
        self.assertEqual(phis[0].aux_str, "res")
        self.assertEqual(len(phis[0].args), 2)
        self.assertEqual(len(merge_block.predecessors), 2)

    def test_06_phi_node_type_mismatch_fails_closed(self):
        stmts = [
            HyIf(
                cond=HyLitBool(True),
                then_stmts=[HyAssign("mismatched", HyLitInt(1))],
                else_stmts=[HyAssign("mismatched", HyLitStr("fail"))],
            ),
        ]
        with self.assertRaises(TypeError):
            self.engine.lower_function("bad_phi", stmts)

    def test_07_while_loop_generates_header_body_and_loop_phis(self):
        stmts = [
            HyAssign("i", HyLitInt(0)),
            HyWhile(
                cond=HyBinary("<", HyIdent("i"), HyLitInt(10)),
                body=[HyAssign("i", HyBinary("+", HyIdent("i"), HyLitInt(1)))],
            ),
            HyReturn(HyIdent("i")),
        ]
        fn = self.engine.lower_function("loop_test", stmts)
        self.assertEqual(len(fn.blocks), 4)
        header = fn.blocks[1]
        phis = [v for v in header.values if v.op == SSAOp.PHI]
        self.assertEqual(len(phis), 1)
        self.assertEqual(phis[0].aux_str, "i")
        self.assertEqual(len(phis[0].args), 2)

    def test_08_channel_send_and_receive_ops(self):
        stmts = [
            HyAssign("ch", HyCall("make_chan", [HyLitInt(1)])),
            HyChanSend(HyIdent("ch"), HyLitInt(42)),
            HyAssign("recv_val", HyChanRecv(HyIdent("ch"))),
            HyReturn(HyIdent("recv_val")),
        ]
        fn = self.engine.lower_function("channel_pipeline", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertIn(SSAOp.CHAN_MAKE, ops)
        self.assertIn(SSAOp.CHAN_SEND, ops)
        self.assertIn(SSAOp.CHAN_RECV, ops)

    def test_09_struct_field_projections(self):
        stmts = [
            HyAssign("user_id", HyField(HyIdent("session"), "id")),
            HyReturn(HyIdent("user_id")),
        ]
        fn = SSAFunction(name="struct_access")
        entry = fn.new_block("b_entry")
        fn.entry = entry
        dummy_session = fn.new_value(SSAOp.LOCAL_ADDR, "*UserSession", aux_str="session")
        entry.add_value(dummy_session)
        env = {"session": dummy_session}
        curr_block, _ = self.engine._lower_stmts(fn, entry, stmts, env)
        ops = [v.op for v in curr_block.values]
        self.assertIn(SSAOp.FIELD_ADDR, ops)
        self.assertIn(SSAOp.LOAD, ops)

    def test_10_immunity_to_go_reserved_keywords(self):
        stmts = [
            HyAssign("interface", HyLitInt(1)),
            HyAssign("select", HyLitInt(2)),
            HyAssign("default", HyBinary("+", HyIdent("interface"), HyIdent("select"))),
            HyReturn(HyIdent("default")),
        ]
        fn = self.engine.lower_function("keyword_immunity", stmts)
        self.assertEqual(len(fn.blocks[0].values), 4)
        for v in fn.blocks[0].values:
            self.assertTrue(v.id > 0)

    def test_11_unused_variables_do_not_produce_build_failures(self):
        stmts = [
            HyAssign("unused_alpha", HyLitInt(100)),
            HyAssign("unused_beta", HyLitStr("discarded")),
            HyReturn(HyLitInt(0)),
        ]
        fn = self.engine.lower_function("unused_vars", stmts)
        ret_val = fn.blocks[0].values[-1]
        self.assertEqual(ret_val.op, SSAOp.RETURN)

    def test_12_single_assignment_invariance(self):
        stmts = [
            HyAssign("x", HyLitInt(10)),
            HyAssign("x", HyLitInt(20)),
            HyAssign("x", HyLitInt(30)),
            HyReturn(HyIdent("x")),
        ]
        fn = self.engine.lower_function("rebind_ssa", stmts)
        consts = [v for v in fn.blocks[0].values if v.op == SSAOp.CONST_INT]
        self.assertEqual(len(consts), 3)
        ids = [v.id for v in consts]
        self.assertEqual(len(ids), len(set(ids)))

    def test_13_ssa_dominance_frontier(self):
        stmts = [
            HyAssign("base", HyLitInt(5)),
            HyIf(
                cond=HyLitBool(True),
                then_stmts=[HyAssign("res", HyBinary("+", HyIdent("base"), HyLitInt(1)))],
                else_stmts=[HyAssign("res", HyBinary("-", HyIdent("base"), HyLitInt(1)))],
            ),
            HyReturn(HyIdent("res")),
        ]
        fn = self.engine.lower_function("dominance", stmts)
        entry_val = fn.blocks[0].values[0]
        self.assertEqual(entry_val.aux_int, 5)

    def test_14_undefined_variable_raises_value_error(self):
        stmts = [
            HyReturn(HyIdent("not_defined")),
        ]
        with self.assertRaises(ValueError):
            self.engine.lower_function("error_fn", stmts)

    def test_15_nested_complex_expressions(self):
        expr = HyBinary(
            "+",
            HyBinary("*", HyLitInt(2), HyLitInt(3)),
            HyBinary("/", HyLitInt(20), HyLitInt(4))
        )
        stmts = [
            HyAssign("result", expr),
            HyReturn(HyIdent("result")),
        ]
        fn = self.engine.lower_function("complex_expr", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertEqual(ops, [SSAOp.CONST_INT, SSAOp.CONST_INT, SSAOp.MUL,
                               SSAOp.CONST_INT, SSAOp.CONST_INT, SSAOp.DIV,
                               SSAOp.ADD, SSAOp.RETURN])

    def test_16_gc_stack_map_bitmaps_for_scalars_vs_pointers(self):
        slots = [
            FrameSlot(name="scalar_count", offset=0, size=8, is_ptr=False, stack_certified=True),
            FrameSlot(name="ptr_chan", offset=8, size=8, is_ptr=True, stack_certified=True),
            FrameSlot(name="scalar_flag", offset=16, size=8, is_ptr=False, stack_certified=True),
            FrameSlot(name="ptr_obj", offset=24, size=8, is_ptr=True, stack_certified=True),
        ]
        mask, bit_str = GCStackMap.generate_bitmap(slots)
        self.assertEqual(mask, 0b1010)
        self.assertEqual(bit_str, "1010")

    def test_17_write_barrier_insertion_under_active_gc_mark(self):
        engine_wb = SSALoweringEngine(write_barriers_active=True)
        stmts = [
            HyAssign("ptr_slot", HyCall("make_chan", [HyLitInt(1)]), is_owned=True, escapes=False, is_ptr=True),
        ]
        fn = engine_wb.lower_function("wb_test", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertIn(SSAOp.STORE_WB, ops)
        self.assertEqual(engine_wb.write_barriers_count, 1)

    def test_18_forced_gc_stress_simulation_safepoints(self):
        slots = [
            FrameSlot(name="x", offset=0, size=8, is_ptr=False, stack_certified=True),
            FrameSlot(name="p", offset=8, size=8, is_ptr=True, stack_certified=True),
        ]
        self.assertTrue(GCStackMap.verify_safepoint_trace(slots, live_ptrs={"p"}))
        self.assertFalse(GCStackMap.verify_safepoint_trace(slots, live_ptrs={"x"}))

    def test_19_cross_task_concurrency_escape_revocation(self):
        stmts = [
            HyAssign("ch", HyCall("make_chan", [HyLitInt(1)])),
            HyAssign("shared_buf", HyLitInt(8192), is_owned=True, escapes=True),
            HyChanSend(HyIdent("ch"), HyIdent("shared_buf")),
        ]
        _ = self.engine.lower_function("task_boundary_escape", stmts)
        self.assertEqual(self.engine.revoked_certificates, 1)
        self.assertEqual(self.engine.heap_allocations, 1)
        self.assertEqual(self.engine.stack_allocations, 0)

    def test_20_toolchain_adapter_version_compatibility_matrix(self):
        stmts = [
            HyAssign("a", HyLitInt(1)),
            HyAssign("b", HyLitInt(2)),
            HyReturn(HyBinary("+", HyIdent("a"), HyIdent("b"))),
        ]
        fn = self.engine.lower_function("abi_compat", stmts)
        for ver in ("1.21", "1.22", "1.26.9"):
            adapter = SSAVersionAdapter(target_go_version=ver)
            self.assertTrue(adapter.validate_abi(fn))

    def test_21_internal_opcode_remapping_absorption(self):
        adapter_old = SSAVersionAdapter(target_go_version="1.21")
        adapter_new = SSAVersionAdapter(target_go_version="1.26.9")
        self.assertEqual(adapter_old.map_op(SSAOp.CONST_INT), "OpConst64")
        self.assertEqual(adapter_new.map_op(SSAOp.CONST_INT), "OpConst64")
        self.assertEqual(adapter_old.map_op(SSAOp.STORE_WB), "OpStore")
        self.assertEqual(adapter_new.map_op(SSAOp.STORE_WB), "OpStoreWB")

    def test_22_defer_unwinding_ssa_phi_convergence(self):
        stmts = [
            HyDefer(HyCall("cleanup", [HyLitInt(0)])),
            HyReturn(HyLitInt(1)),
        ]
        fn = self.engine.lower_function("defer_unwind", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertIn(SSAOp.DEFER, ops)
        self.assertIn(SSAOp.RETURN, ops)

    def test_23_select_with_default_nonblocking_lowering(self):
        stmts = [
            HySelect(
                cases=[
                    (HyChanSend(HyIdent("ch"), HyLitInt(10)), [HyAssign("sent", HyLitBool(True))])
                ],
                default_stmts=[HyAssign("sent", HyLitBool(False))]
            ),
            HyReturn(HyIdent("sent")),
        ]
        fn = SSAFunction(name="select_nonblocking")
        entry = fn.new_block("b_entry")
        fn.entry = entry
        dummy_ch = fn.new_value(SSAOp.CHAN_MAKE, "chan[int64]", aux_str="ch")
        entry.add_value(dummy_ch)
        env = {"ch": dummy_ch}
        curr_block, res_env = self.engine._lower_stmts(fn, entry, stmts, env)
        self.assertIn("sent", res_env)
        block_names = [b.name for b in fn.blocks]
        self.assertTrue(any("select" in n for n in block_names))
        self.assertTrue(any("default" in n for n in block_names))

    def test_24_nested_loops_with_labeled_break_backedges(self):
        inner_loop = HyWhile(
            cond=HyLitBool(True),
            body=[HyBreak(label="outer")]
        )
        outer_loop = HyWhile(
            cond=HyLitBool(True),
            body=[inner_loop]
        )
        fn = self.engine.lower_function("nested_breaks", [outer_loop])
        all_ops = [v.op for b in fn.blocks for v in b.values]
        self.assertIn(SSAOp.BREAK, all_ops)

    def test_25_mixed_struct_field_partial_stack_certification(self):
        engine_wb = SSALoweringEngine(write_barriers_active=True)
        stmts = [
            HyStructAlloc(
                var_name="session",
                fields=[
                    ("id", HyLitInt(1001), False),
                    ("token_ptr", HyLitStr("secure_token"), True),
                ],
                is_owned=True,
                escapes=False
            )
        ]
        fn = engine_wb.lower_function("mixed_struct", stmts)
        self.assertEqual(engine_wb.stack_allocations, 1)
        self.assertEqual(len(fn.frame_slots), 2)
        mask, bit_str = GCStackMap.generate_bitmap(fn.frame_slots)
        self.assertEqual(mask, 0b10)
        self.assertEqual(bit_str, "10")
        self.assertEqual(engine_wb.write_barriers_count, 1)


if __name__ == "__main__":
    unittest.main()
