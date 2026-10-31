#!/usr/bin/env python3
"""
verifier.py — Lightweight Symbolic Commutativity & Purity Checker
================================================================
Prototype static law-check engine for Go-Hybrid Wildcard Proposal #2385:
"Certified Commutative Merge Capsules (merge[M])".

Verifies that a reducer function `fn (a *M) combine(move b M)` satisfies:
  1. Purity: No external captures, I/O, or side-effecting operations.
  2. Commutativity: combine(combine(x, a), b) == combine(combine(x, b), a)
  3. Associativity: combine(combine(a, b), c) == combine(a, combine(b, c))

Uses symbolic term rewriting and AC (associative-commutative) flattening.
Latency target: < 5ms per function check (zero external SMT solver).
"""

import ast
import sys
import time
from typing import Dict, Any, Tuple, Optional, List, Set, Union

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")


class VerificationError(Exception):
    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(f"[{code}] {message}")


COMMUTATIVE_OPS = {"+", "*", "|", "&", "^", "max", "min"}

DISALLOWED_CALLS = {
    "print", "println", "fmt.println", "fmt.printf", "fmt.print",
    "os.exit", "time.now", "make", "panic", "log.print"
}


class SymExpr:
    def __init__(self, op: Optional[str] = None, args: Optional[List['SymExpr']] = None, val: Optional[str] = None):
        self.op = op
        self.val = val
        self.args: List['SymExpr'] = []

        if op and args is not None:
            flat_args = []
            for arg in args:
                if op in COMMUTATIVE_OPS and arg.op == op:
                    flat_args.extend(arg.args)
                else:
                    flat_args.append(arg)
            if op in COMMUTATIVE_OPS:
                flat_args.sort(key=lambda x: repr(x))
            self.args = flat_args

    @staticmethod
    def leaf(name: str) -> 'SymExpr':
        return SymExpr(val=name)

    @staticmethod
    def bin_op(op: str, left: 'SymExpr', right: 'SymExpr') -> 'SymExpr':
        return SymExpr(op=op, args=[left, right])

    def __repr__(self) -> str:
        if self.val is not None:
            return self.val
        if self.op in ("max", "min"):
            return f"{self.op}({', '.join(repr(a) for a in self.args)})"
        if len(self.args) == 2 and self.op not in COMMUTATIVE_OPS:
            return f"({repr(self.args[0])} {self.op} {repr(self.args[1])})"
        return f"({f' {self.op} '.join(repr(a) for a in self.args)})"

    def __eq__(self, other: Any) -> bool:
        if not isinstance(other, SymExpr):
            return False
        return repr(self) == repr(other)


class SymbolicState:
    def __init__(self, name: str):
        self.name = name
        self.prefix = name
        self.fields: Dict[str, SymExpr] = {}

    def get_field(self, field: str) -> SymExpr:
        return self.fields.get(field, SymExpr.leaf(f"{self.name}.{field}"))

    def set_field(self, field: str, expr: SymExpr):
        self.fields[field] = expr

    def copy(self) -> 'SymbolicState':
        cp = SymbolicState(self.name)
        cp.fields = dict(self.fields)
        return cp


class CommutativityVerifier(ast.NodeVisitor):
    def __init__(self, func_src: str):
        self.src = func_src
        self.tree = ast.parse(func_src)
        self.func_def: Optional[ast.FunctionDef] = None
        for node in ast.walk(self.tree):
            if isinstance(node, ast.FunctionDef):
                self.func_def = node
                break
        if not self.func_def:
            raise VerificationError("HYB-E3300", "No function definition found in source")

        self.param_a = self.func_def.args.args[0].arg if len(self.func_def.args.args) > 0 else "a"
        self.param_b = self.func_def.args.args[1].arg if len(self.func_def.args.args) > 1 else "b"

    def check_purity(self):
        for node in ast.walk(self.func_def):
            if isinstance(node, ast.Call):
                func_name = ""
                if isinstance(node.func, ast.Name):
                    func_name = node.func.id
                elif isinstance(node.func, ast.Attribute):
                    base = node.func.value.id if isinstance(node.func.value, ast.Name) else ""
                    func_name = f"{base}.{node.func.attr}" if base else node.func.attr

                if func_name in ("min", "max"):
                    continue

                if func_name in DISALLOWED_CALLS or any(dis in func_name for dis in ("print", "write", "send")):
                    raise VerificationError(
                        "HYB-E3302",
                        f"Side-effecting call '{func_name}' is not allowed in pure merge combiner"
                    )

            if isinstance(node, (ast.Yield, ast.YieldFrom, ast.AsyncFunctionDef)):
                raise VerificationError("HYB-E3302", "Combiner must be synchronous and pure")

    def _eval_expr(self, node: ast.AST, other: Union[str, SymbolicState], state: SymbolicState) -> SymExpr:
        if isinstance(node, ast.Attribute):
            base = node.value.id if isinstance(node.value, ast.Name) else ""
            if base == self.param_b:
                if isinstance(other, str):
                    return SymExpr.leaf(f"{other}.{node.attr}")
                else:
                    return other.get_field(node.attr)
            elif base == self.param_a:
                return state.get_field(node.attr)
            return SymExpr.leaf(f"{base}.{node.attr}")

        elif isinstance(node, ast.Name):
            if node.id == self.param_b:
                if isinstance(other, str):
                    return SymExpr.leaf(other)
                else:
                    return other.get_field("val")
            elif node.id == self.param_a:
                return state.get_field("val")
            return SymExpr.leaf(node.id)

        elif isinstance(node, ast.Constant):
            return SymExpr.leaf(str(node.value))

        elif isinstance(node, ast.BinOp):
            op_map = {
                ast.Add: "+", ast.Sub: "-", ast.Mult: "*", ast.Div: "/",
                ast.BitOr: "|", ast.BitAnd: "&", ast.BitXor: "^",
                ast.LShift: "<<", ast.RShift: ">>", ast.Mod: "%"
            }
            op_sym = op_map.get(type(node.op), "?")
            left = self._eval_expr(node.left, other, state)
            right = self._eval_expr(node.right, other, state)
            return SymExpr.bin_op(op_sym, left, right)

        elif isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id in ("max", "min"):
                args = [self._eval_expr(arg, other, state) for arg in node.args]
                return SymExpr(op=node.func.id, args=args)

        return SymExpr.leaf("unsupported")

    def _apply_statements(self, state: SymbolicState, other: Union[str, SymbolicState]) -> SymbolicState:
        st = state.copy()
        other_prefix = other if isinstance(other, str) else other.prefix
        for stmt in self.func_def.body:
            if isinstance(stmt, ast.AugAssign):
                target_field = self._get_target_field(stmt.target)
                if any(kw in target_field.lower() for kw in ("text", "str", "slice", "buf", "log")):
                    raise VerificationError(
                        "HYB-E3301",
                        f"Concatenation on '{target_field}' is non-commutative; requires spawn-indexed fold[M]"
                    )
                op_map = {
                    ast.Add: "+", ast.Mult: "*", ast.BitOr: "|",
                    ast.BitAnd: "&", ast.BitXor: "^"
                }
                if type(stmt.op) not in op_map:
                    raise VerificationError(
                        "HYB-E3301",
                        f"Non-commutative operator '{type(stmt.op).__name__}' on field '{target_field}'"
                    )
                op_sym = op_map[type(stmt.op)]
                val_expr = self._eval_expr(stmt.value, other, st)
                curr_expr = st.get_field(target_field)
                st.set_field(target_field, SymExpr.bin_op(op_sym, curr_expr, val_expr))

            elif isinstance(stmt, ast.Assign):
                if len(stmt.targets) != 1:
                    raise VerificationError("HYB-E3301", "Multi-assignment not supported in reducer")
                target_field = self._get_target_field(stmt.targets[0])
                val_expr = self._eval_expr(stmt.value, other, st)

                if any(kw in target_field.lower() for kw in ("text", "str", "slice", "buf", "log")):
                    if val_expr.op == "+":
                        raise VerificationError(
                            "HYB-E3301",
                            f"Concatenation on '{target_field}' is non-commutative; requires spawn-indexed fold[M]"
                        )

                if val_expr.val == f"{other_prefix}.{target_field}":
                    raise VerificationError(
                        "HYB-E3301",
                        f"Non-commutative overwrite on field '{target_field}': order dependent assignment"
                    )

                if val_expr.op and val_expr.op not in COMMUTATIVE_OPS:
                    raise VerificationError(
                        "HYB-E3301",
                        f"Non-commutative binary operator '{val_expr.op}' in assignment to '{target_field}'"
                    )

                st.set_field(target_field, val_expr)

            elif isinstance(stmt, ast.If):
                if isinstance(stmt.test, ast.Compare):
                    left = self._eval_expr(stmt.test.left, other, st)
                    right = self._eval_expr(stmt.test.comparators[0], other, st)
                    op = stmt.test.ops[0]
                    if isinstance(op, ast.Gt) and len(stmt.body) == 1 and isinstance(stmt.body[0], ast.Assign):
                        tgt = self._get_target_field(stmt.body[0].targets[0])
                        assigned = self._eval_expr(stmt.body[0].value, other, st)
                        curr = st.get_field(tgt)
                        if assigned == left:
                            st.set_field(tgt, SymExpr(op="max", args=[curr, assigned]))
                        else:
                            raise VerificationError("HYB-E3301", "Unrecognized asymmetric condition in reducer")
                    else:
                        raise VerificationError("HYB-E3301", "Conditional update is not a canonical semilattice")

        return st

    def _get_target_field(self, node: ast.AST) -> str:
        if isinstance(node, ast.Attribute):
            return node.attr
        elif isinstance(node, ast.Name):
            return node.id
        return "val"

    def verify(self) -> Tuple[bool, str, float]:
        t0 = time.perf_counter()
        try:
            self.check_purity()

            # 1. Commutativity Check: A ⊕ B == B ⊕ A
            state_a = SymbolicState("A")
            state_b = SymbolicState("B")
            state_ab = self._apply_statements(state_a, "B")
            state_ba = self._apply_statements(state_b, "A")

            comm_diffs = []
            for field in set(state_ab.fields.keys()).union(state_ba.fields.keys()):
                ab_v = state_ab.fields.get(field)
                ba_v = state_ba.fields.get(field)
                if ab_v != ba_v:
                    comm_diffs.append(f"{field}: {ab_v} != {ba_v}")

            if comm_diffs:
                dur = (time.perf_counter() - t0) * 1000
                return False, f"Commutativity check failed: {', '.join(comm_diffs)}", dur

            # 2. Associativity Check: (A ⊕ B) ⊕ C == A ⊕ (B ⊕ C)
            state_abc_left = self._apply_statements(state_ab, "C")
            state_c = SymbolicState("C")
            state_bc = self._apply_statements(state_b, "C")
            state_abc_right = self._apply_statements(state_a, state_bc)

            assoc_diffs = []
            for field in set(state_abc_left.fields.keys()).union(state_abc_right.fields.keys()):
                left_v = state_abc_left.fields.get(field)
                right_v = state_abc_right.fields.get(field)
                if left_v != right_v:
                    assoc_diffs.append(f"{field}: {left_v} != {right_v}")

            if assoc_diffs:
                dur = (time.perf_counter() - t0) * 1000
                return False, f"Associativity check failed: {', '.join(assoc_diffs)}", dur

            dur = (time.perf_counter() - t0) * 1000
            return True, "VERIFIED: Associative and Commutative monoid/semilattice", dur

        except VerificationError as ve:
            dur = (time.perf_counter() - t0) * 1000
            return False, f"{ve.code}: {ve.message}", dur


def verify_reducer(source: str) -> Tuple[bool, str, float]:
    verifier = CommutativityVerifier(source)
    return verifier.verify()
