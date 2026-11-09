import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ssa_lowering import (
    SSALoweringEngine, SSAOp,
    HyLitInt, HyLitFloat, HyLitStr, HyLitBool, HyIdent, HyBinary, HyCall,
    HyField, HyChanRecv, HyChanSend, HyAssign, HyIf, HyWhile, HyReturn
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
        head_block = fn.blocks[1]
        phis = [v for v in head_block.values if v.op == SSAOp.PHI]
        self.assertEqual(len(phis), 1)
        self.assertEqual(phis[0].aux_str, "i")
        self.assertEqual(len(phis[0].args), 2)

    def test_08_channel_send_and_receive_ops(self):
        stmts = [
            HyAssign("ch", HyCall("make_chan", [])),
            HyChanSend(HyIdent("ch"), HyLitInt(99)),
            HyAssign("msg", HyChanRecv(HyIdent("ch"))),
            HyReturn(HyIdent("msg")),
        ]
        fn = self.engine.lower_function("chan_pipe", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertIn(SSAOp.CHAN_MAKE, ops)
        self.assertIn(SSAOp.CHAN_SEND, ops)
        self.assertIn(SSAOp.CHAN_RECV, ops)

    def test_09_struct_field_projections(self):
        stmts = [
            HyAssign("user", HyLitInt(1)),
            HyAssign("age", HyField(HyIdent("user"), "Age")),
            HyReturn(HyIdent("age")),
        ]
        fn = self.engine.lower_function("struct_proj", stmts)
        ops = [v.op for v in fn.blocks[0].values]
        self.assertIn(SSAOp.FIELD_ADDR, ops)
        self.assertIn(SSAOp.LOAD, ops)

    def test_10_immunity_to_go_reserved_keywords(self):
        stmts = [
            HyAssign("interface", HyLitStr("safe")),
            HyAssign("default", HyLitInt(10)),
            HyAssign("select", HyLitBool(False)),
            HyAssign("match", HyBinary("+", HyIdent("default"), HyLitInt(5))),
            HyReturn(HyIdent("match")),
        ]
        fn = self.engine.lower_function("keyword_immunity", stmts)
        self.assertEqual(self.engine.syntax_errors, 0)
        self.assertEqual(fn.blocks[0].values[-1].op, SSAOp.RETURN)

    def test_11_unused_variables_do_not_produce_build_failures(self):
        stmts = [
            HyAssign("unused_var_1", HyLitInt(100)),
            HyAssign("unused_var_2", HyLitStr("abandoned")),
            HyReturn(HyLitInt(42)),
        ]
        fn = self.engine.lower_function("unused_safe", stmts)
        self.assertEqual(self.engine.syntax_errors, 0)
        self.assertEqual(fn.blocks[0].values[-1].op, SSAOp.RETURN)

    def test_12_single_assignment_invariance(self):
        stmts = [
            HyAssign("x", HyLitInt(1)),
            HyAssign("x", HyLitInt(2)),
            HyAssign("x", HyLitInt(3)),
            HyReturn(HyIdent("x")),
        ]
        fn = self.engine.lower_function("ssa_ssa", stmts)
        all_ids = [v.id for b in fn.blocks for v in b.values]
        self.assertEqual(len(all_ids), len(set(all_ids)))
        ret_val = fn.blocks[0].values[-1]
        self.assertEqual(ret_val.args[0].id, 3)

    def test_13_ssa_dominance_frontier(self):
        stmts = [
            HyAssign("a", HyLitInt(5)),
            HyAssign("b", HyLitInt(7)),
            HyAssign("c", HyBinary("*", HyIdent("a"), HyIdent("b"))),
        ]
        fn = self.engine.lower_function("dominance", stmts)
        b = fn.blocks[0]
        c_val = b.values[-1]
        self.assertEqual(c_val.op, SSAOp.MUL)
        self.assertLess(b.values.index(c_val.args[0]), b.values.index(c_val))
        self.assertLess(b.values.index(c_val.args[1]), b.values.index(c_val))

    def test_14_undefined_variable_raises_value_error(self):
        stmts = [
            HyAssign("y", HyBinary("+", HyIdent("unbound_x"), HyLitInt(1))),
        ]
        with self.assertRaises(ValueError):
            self.engine.lower_function("unbound_var", stmts)

    def test_15_nested_complex_expressions(self):
        stmts = [
            HyAssign("a", HyLitInt(10)),
            HyAssign("b", HyLitInt(20)),
            HyAssign("c", HyLitInt(30)),
            HyAssign("d", HyLitInt(5)),
            HyAssign("sum", HyBinary("+", HyIdent("a"), HyIdent("b"))),
            HyAssign("diff", HyBinary("-", HyIdent("c"), HyIdent("d"))),
            HyAssign("prod", HyBinary("*", HyIdent("sum"), HyIdent("diff"))),
            HyAssign("res", HyBinary("/", HyIdent("prod"), HyLitInt(2))),
            HyReturn(HyIdent("res")),
        ]
        fn = self.engine.lower_function("complex_expr", stmts)
        self.assertEqual(len(fn.blocks), 1)
        self.assertEqual(fn.blocks[0].values[-1].op, SSAOp.RETURN)


if __name__ == "__main__":
    unittest.main()
