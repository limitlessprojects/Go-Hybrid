"""W-1933 (#1933): Structured Go AST Emission + Self-Validation Gate."""

from __future__ import annotations
from dataclasses import dataclass
from typing import List, Set, Tuple


@dataclass(frozen=True)
class GoStmtNode:
    kind: str  # "short_decl" | "select_recv_case" | "expr_call" | "raw_invalid_tokens"
    lhs: str = ""
    rhs: str = ""
    body: Tuple["GoStmtNode", ...] = ()
    hy_file: str = "main.hy"
    hy_line: int = 1
    wrap_case_block: bool = True


@dataclass(frozen=True)
class EmitValidationResult:
    valid: bool
    code: str
    emitted_go: str
    origin_hy_span: str


def emit_and_validate_go_ast(pkg_name: str, fn_name: str, stmts: List[GoStmtNode]) -> EmitValidationResult:
    """Emit structured Go AST nodes and run the pre-write syntax & lexical-scope gate."""
    lines = [f"package {pkg_name}", "", f"func {fn_name}() {{"]
    outer_decls: Set[str] = set()

    for st in stmts:
        span_tag = f"{st.hy_file}:{st.hy_line}"
        if st.kind == "raw_invalid_tokens":
            return EmitValidationResult(False, "W1933_ERR_INVALID_AST_TOKEN_SEQ", "", span_tag)

        if st.kind == "short_decl":
            if st.lhs in outer_decls:
                return EmitValidationResult(False, "W1933_ERR_NO_NEW_VARS_SHORT_DECL", "", span_tag)
            outer_decls.add(st.lhs)
            lines.append(f"\t//line {st.hy_file}:{st.hy_line}")
            lines.append(f"\t{st.lhs} := {st.rhs}")

        elif st.kind == "select_recv_case":
            lines.append("\tselect {")
            if st.wrap_case_block:
                # Structured emitter wraps arm body in explicit lexical block { ... } so #1769 shadows cleanly
                lines.append(f"\tcase {st.lhs} := <-{st.rhs}: {{")
                inner_decls: Set[str] = set()
                for child in st.body:
                    if child.kind == "short_decl":
                        if child.lhs in inner_decls:
                            return EmitValidationResult(
                                False, "W1933_ERR_NO_NEW_VARS_SHORT_DECL", "", f"{child.hy_file}:{child.hy_line}"
                            )
                        inner_decls.add(child.lhs)
                        lines.append(f"\t\t//line {child.hy_file}:{child.hy_line}")
                        lines.append(f"\t\t{child.lhs} := {child.rhs}")
                lines.append("\t}")
            else:
                # Unwrapped string-template emission (#1769 bug): case header and body share flat scope
                case_decls = {st.lhs}
                lines.append(f"\tcase {st.lhs} := <-{st.rhs}:")
                for child in st.body:
                    if child.kind == "short_decl" and child.lhs in case_decls:
                        return EmitValidationResult(
                            False,
                            "W1933_ERR_SELECT_ARM_REDECL_1769",
                            "",
                            f"{child.hy_file}:{child.hy_line}",
                        )
            lines.append("\t}")

        elif st.kind == "expr_call":
            lines.append(f"\t//line {st.hy_file}:{st.hy_line}")
            lines.append(f"\t{st.rhs}")

    lines.append("}")
    return EmitValidationResult(True, "OK", "\n".join(lines), f"{stmts[0].hy_file}:1" if stmts else "main.hy:1")
