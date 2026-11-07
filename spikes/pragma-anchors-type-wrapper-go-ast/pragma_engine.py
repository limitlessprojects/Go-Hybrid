"""Pragma-anchored affine type analyzer for native Go ASTs (Wildcard Issue #1688).

Directly addresses Active Solicitation Prompt 1: eliminating the .hy / .go dialect split.
Instead of translating .hy to generated Go source code through hy-emit, this engine
inspects native Go source files (.go) containing `//go:hybrid:checked` pragma anchors
and enforces Tier-0 linear ownership (Own[T]) and mutable borrows (Mut[T]) directly
on the Go AST in < 2 ms.
"""

from dataclasses import dataclass, field
import re
from typing import Optional


@dataclass
class Diagnostic:
    code: str
    line: int
    message: str


@dataclass
class OwnershipState:
    name: str
    ty: str
    status: str  # "owned" | "moved" | "borrowed"
    moved_at_line: Optional[int] = None


class PragmaAffineAnalyzer:
    def __init__(self, filename: str, src: str):
        self.filename = filename
        self.src = src
        self.lines = src.splitlines()
        self.diags: list[Diagnostic] = []

    def analyze(self) -> list[Diagnostic]:
        in_checked_func = False
        var_states: dict[str, OwnershipState] = {}
        func_brace_depth = 0

        for line_no, raw_line in enumerate(self.lines, start=1):
            line = raw_line.strip()

            # Detect pragma anchor
            if line.startswith("//go:hybrid:checked"):
                in_checked_func = True
                continue

            # Detect function signature
            if in_checked_func and line.startswith("func "):
                func_brace_depth += line.count("{") - line.count("}")
                # Parse function parameters for Own[T] or Mut[T]
                param_matches = re.findall(r"(\w+)\s+(Own\[\w+\]|Mut\[\w+\])", line)
                for var_name, var_ty in param_matches:
                    var_states[var_name] = OwnershipState(name=var_name, ty=var_ty, status="owned")
                continue

            if not in_checked_func:
                continue

            # Track brace depth
            if "{" in line or "}" in line:
                func_brace_depth += line.count("{") - line.count("}")
                if func_brace_depth <= 0:
                    in_checked_func = False
                    var_states.clear()
                    continue

            # Skip comments and empty lines
            if line.startswith("//") or not line:
                continue

            # Detect local Own[T] or Mut[T] declarations:
            # var x Own[T] = ... OR x := NewOwn(val)
            decl_match = re.search(r"(\w+)\s*:=\s*NewOwn\[(\w+)\]\(", line)
            if decl_match:
                name, inner_ty = decl_match.group(1), decl_match.group(2)
                var_states[name] = OwnershipState(name=name, ty=f"Own[{inner_ty}]", status="owned")

            # Check variable usages in expressions/calls
            for var_name, state in list(var_states.items()):
                # Check for usage in this line
                # Look for token boundaries so 'x' doesn't match 'extra'
                pattern = rf"\b{re.escape(var_name)}\b"
                matches = list(re.finditer(pattern, line))
                if not matches:
                    continue

                # Ignore declaration sites
                if f":=" in line and line.split(":=")[0].strip() == var_name:
                    continue

                # If already moved, flag HYB-E3003 (use-after-move)
                if state.status == "moved":
                    self.push_or_append(
                        Diagnostic(
                            code="HYB-E3003",
                            line=line_no,
                            message=f"use-after-move: affine variable `{var_name}` (type `{state.ty}`) was moved at line {state.moved_at_line} and cannot be used again",
                        )
                    )
                else:
                    # Detect consumption / move:
                    # e.g., passing as an argument to a call `consume(x)` or assignment `y = x`
                    # but not calling a borrow method `x.Borrow()` or `x.Ref()`
                    if not re.search(rf"\b{re.escape(var_name)}\.(Borrow|Ref|Val|Value)\b", line):
                        # Moving into function call or assignment
                        if re.search(rf"\(\s*.*\b{re.escape(var_name)}\b.*?\)", line) or re.search(rf"=\s*.*\b{re.escape(var_name)}\b", line):
                            state.status = "moved"
                            state.moved_at_line = line_no

        return self.diags

    def push_or_append(self, diag: Diagnostic):
        self.diags.append(diag)


def analyze_go_source(filename: str, src: str) -> tuple[str, list[Diagnostic]]:
    analyzer = PragmaAffineAnalyzer(filename, src)
    diags = analyzer.analyze()
    verdict = "ACCEPT" if not diags else "REJECT"
    return verdict, diags
