"""Unit tests for W-1933 (#1933): Structured Go AST Emission + Self-Validation Gate."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from structured_emit_gate import GoStmtNode, emit_and_validate_go_ast  # noqa: E402


class TestW1933StructuredEmitGate(unittest.TestCase):
    def test_01_structured_select_case_block_prevents_issue_1769_redecl_error(self):
        stmts = [
            GoStmtNode(
                "select_recv_case", lhs="v", rhs="ch",
                body=(GoStmtNode("short_decl", lhs="v", rhs="v + 1", hy_file="sel1.hy", hy_line=11),),
                hy_file="sel1.hy", hy_line=10, wrap_case_block=True,
            )
        ]
        res = emit_and_validate_go_ast("main", "main", stmts)
        self.assertTrue(res.valid)
        self.assertIn("case v := <-ch: {", res.emitted_go)
        self.assertIn("//line sel1.hy:11", res.emitted_go)

    def test_02_unwrapped_string_template_select_caught_by_prewrite_gate(self):
        stmts = [
            GoStmtNode(
                "select_recv_case", lhs="v", rhs="ch",
                body=(GoStmtNode("short_decl", lhs="v", rhs="v + 1", hy_file="sel1.hy", hy_line=11),),
                hy_file="sel1.hy", hy_line=10, wrap_case_block=False,
            )
        ]
        res = emit_and_validate_go_ast("main", "main", stmts)
        self.assertFalse(res.valid)
        self.assertEqual(res.code, "W1933_ERR_SELECT_ARM_REDECL_1769")
        self.assertEqual(res.origin_hy_span, "sel1.hy:11")

    def test_03_duplicate_short_decl_in_same_scope_caught_with_hy_line(self):
        stmts = [
            GoStmtNode("short_decl", lhs="x", rhs="1", hy_file="app.hy", hy_line=4),
            GoStmtNode("short_decl", lhs="x", rhs="2", hy_file="app.hy", hy_line=5),
        ]
        res = emit_and_validate_go_ast("main", "main", stmts)
        self.assertFalse(res.valid)
        self.assertEqual(res.code, "W1933_ERR_NO_NEW_VARS_SHORT_DECL")
        self.assertEqual(res.origin_hy_span, "app.hy:5")

    def test_04_invalid_token_sequence_rejected_before_disk_write(self):
        res = emit_and_validate_go_ast("main", "main", [GoStmtNode("raw_invalid_tokens", hy_file="bad.hy", hy_line=9)])
        self.assertFalse(res.valid)
        self.assertEqual(res.code, "W1933_ERR_INVALID_AST_TOKEN_SEQ")


if __name__ == "__main__":
    unittest.main()
