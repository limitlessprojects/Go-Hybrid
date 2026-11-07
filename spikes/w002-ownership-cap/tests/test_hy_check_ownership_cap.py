import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))
from tools.hy_check_ownership_cap import check_program


class TestW002TypeCarriedOwnership(unittest.TestCase):
    def test_01_corpus_15_chunk_of_loop_accepted_without_call_local_roots(self):
        res = check_program([
            {"op": "let", "name": "src", "rhs_kind": "alloc"},
            {"op": "loop", "body": [
                {"op": "let", "name": "chunk", "rhs_kind": "call", "rhs_arg": "chunk_of:src"},
                {"op": "move_sink", "target": "chunk"},
            ]},
        ])
        self.assertTrue(res.accepted)
        self.assertEqual(res.deleted_tables_used, 0)

    def test_02_issue_194_loop_alias_move_rejected_e3001(self):
        res = check_program([
            {"op": "let", "name": "src", "rhs_kind": "alloc"},
            {"op": "loop", "body": [
                {"op": "let", "name": "alias_chunk", "rhs_kind": "call", "rhs_arg": "identity_slice:src"},
                {"op": "move_sink", "target": "alias_chunk"},
            ]},
        ])
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostics[0].code, "E3001")

    def test_03_issue_14_31_lexical_shadow_move_preserves_outer_own(self):
        res = check_program([
            {"op": "let", "name": "x", "rhs_kind": "alloc"},
            {"op": "block", "body": [
                {"op": "let", "name": "x", "rhs_kind": "alloc"},
                {"op": "move_sink", "target": "x"},
            ]},
            {"op": "move_sink", "target": "x"},
        ])
        self.assertTrue(res.accepted)

    def test_04_issue_73_160_mutation_while_borrowed_rejected_e3003(self):
        res = check_program([
            {"op": "let", "name": "buf", "rhs_kind": "alloc"},
            {"op": "let", "name": "view", "rhs_kind": "borrow_var", "rhs_arg": "buf"},
            {"op": "mutate", "target": "buf"},
        ])
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostics[0].code, "E3003")

    def test_05_issue_195_borrow_region_escape_rejected_e3004(self):
        res = check_program([
            {"op": "let", "name": "holder", "rhs_kind": "alloc"},
            {"op": "block", "body": [
                {"op": "let", "name": "local_buf", "rhs_kind": "alloc"},
                {"op": "let", "name": "b", "rhs_kind": "borrow_var", "rhs_arg": "local_buf"},
                {"op": "assign_outer", "outer": "holder", "inner": "b"},
            ]},
        ])
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostics[0].code, "E3004")

    def test_06_issue_188_move_non_owned_subslice_rejected_e3101(self):
        res = check_program([
            {"op": "let", "name": "buf", "rhs_kind": "alloc"},
            {"op": "let", "name": "sub", "rhs_kind": "call", "rhs_arg": "subslice_view:buf"},
            {"op": "move_sink", "target": "sub"},
        ])
        self.assertFalse(res.accepted)
        self.assertEqual(res.diagnostics[0].code, "E3101")

    def test_07_closure_capture_semantics_e3006_and_e3001(self):
        unshared = check_program([
            {"op": "let", "name": "payload", "rhs_kind": "alloc"},
            {"op": "closure_spawn", "captures": ["payload"], "is_move": False},
        ])
        self.assertFalse(unshared.accepted)
        self.assertEqual(unshared.diagnostics[0].code, "E3006")

        moved_reuse = check_program([
            {"op": "let", "name": "payload", "rhs_kind": "alloc"},
            {"op": "closure_spawn", "captures": ["payload"], "is_move": True},
            {"op": "move_sink", "target": "payload"},
        ])
        self.assertFalse(moved_reuse.accepted)
        self.assertEqual(moved_reuse.diagnostics[0].code, "E3001")


if __name__ == "__main__":
    unittest.main()
