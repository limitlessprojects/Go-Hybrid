#!/usr/bin/env python3
"""
W-033 Ghost Capability Static Gate Verifier
Directly parses standard Go source files and attaches Hybrid ownership/borrowing
contract tokens via compiler-friendly comment pragmas (//hy:move, //hy:shared, etc.).
"""

import sys
import re
from typing import Dict, List, Tuple, Optional, Set
from dataclasses import dataclass, field
from enum import Enum


class GhostState(Enum):
    OWNED = "owned"          # Exclusive linear ownership
    MOVED = "moved"          # Ownership transferred; subsequent use forbidden (HYB-E1001)
    SHARED = "shared"        # Immutable shared reference across tasks (SendCap)
    BORROWED = "borrowed"    # Mutable scoped borrow
    REGION_LOCAL = "region"  # Bound to off-heap or task-local region (HYB-E4001 on escape)


@dataclass
class Diagnostic:
    code: str
    message: str
    line: int
    variable: str


@dataclass
class ScopeFrame:
    kind: str  # 'package', 'function', 'block', 'goroutine', 'region'
    declared: Set[str] = field(default_factory=set)
    region_id: Optional[str] = None


class GhostCapabilityGate:
    """
    Static verifier for inline Go Ghost-Capability pragmas.
    """

    def __init__(self):
        self.diagnostics: List[Diagnostic] = []

    def _strip_strings(self, code: str) -> str:
        """
        Masks string literals to prevent syntax poisoning while preserving comment pragmas and line numbers.
        """
        code = re.sub(r"`[^`]*`", lambda m: '""' + ("\n" * m.group(0).count("\n")), code, flags=re.DOTALL)
        code = re.sub(r'"(?:\\.|[^"\\])*"', '""', code)
        return code

    def check_program(self, code: str) -> List[Diagnostic]:
        self.diagnostics = []
        clean_code = self._strip_strings(code)
        raw_lines = clean_code.splitlines()

        # Check for package-level opt-in: //hy:check
        has_hy_check = any("//hy:check" in line for line in raw_lines[:15])

        variables: Dict[str, GhostState] = {}
        var_regions: Dict[str, str] = {}
        aliases: Dict[str, str] = {}

        scope_stack: List[ScopeFrame] = [ScopeFrame(kind='package')]
        active_pragmas_for_next_stmt: List[Tuple[str, str]] = []  # [(directive, target)]

        for idx, raw_line in enumerate(raw_lines, 1):
            line = raw_line.strip()
            if not line:
                continue

            # 1. Extract and consume comment pragmas: //hy:directive [args]
            pragma_matches = re.findall(r"//hy:([a-zA-Z0-9_]+)(?:\[([a-zA-Z0-9_]+)\])?(?:\s+([a-zA-Z0-9_\.,\s]+))?", line)
            for p_name, p_bracket, p_args in pragma_matches:
                directive = p_name.lower()
                arg = (p_bracket or p_args or "").strip()
                if directive in ("move", "shared", "borrow", "region", "task"):
                    active_pragmas_for_next_stmt.append((directive, arg))

            # Strip comments after extracting pragmas
            code_part = line
            if "//" in code_part:
                code_part = code_part.split("//", 1)[0].strip()

            if not code_part:
                continue

            # 2. Block structure tracking
            is_go_stmt = bool(re.search(r"\bgo\s+(func|\w+)", code_part))
            is_region_start = any(d == "region" for d, _ in active_pragmas_for_next_stmt)
            region_name = next((arg for d, arg in active_pragmas_for_next_stmt if d == "region"), None)

            for char in code_part:
                if char == '{':
                    kind = 'block'
                    if is_go_stmt:
                        kind = 'goroutine'
                        is_go_stmt = False
                    elif is_region_start:
                        kind = 'region'
                        is_region_start = False

                    scope_stack.append(ScopeFrame(kind=kind, region_id=region_name))
                elif char == '}':
                    if len(scope_stack) > 1:
                        popped = scope_stack.pop()
                        for v in popped.declared:
                            variables.pop(v, None)
                            var_regions.pop(v, None)

            in_goroutine = any(f.kind == 'goroutine' for f in scope_stack)
            current_region = next((f.region_id for f in reversed(scope_stack) if f.region_id), None)

            # 3. Variable declarations: `x := ...` or `var x T`
            decl_match = re.search(r"\b([a-zA-Z0-9_]+)\s*:=\s*(.+)$", code_part)
            if decl_match:
                var_name = decl_match.group(1).strip()
                rhs = decl_match.group(2).strip()

                scope_stack[-1].declared.add(var_name)
                variables[var_name] = GhostState.OWNED
                aliases[var_name] = var_name

                if current_region:
                    variables[var_name] = GhostState.REGION_LOCAL
                    var_regions[var_name] = current_region

                # Check if RHS reads a moved variable
                for v, state in list(variables.items()):
                    if v != var_name and re.search(rf"\b{v}\b", rhs):
                        canon = aliases.get(v, v)
                        if variables.get(canon) == GhostState.MOVED:
                            self.diagnostics.append(
                                Diagnostic(
                                    code="HYB-E1001",
                                    message=f"use of moved value '{v}'; ownership was transferred",
                                    line=idx,
                                    variable=v
                                )
                            )

            # 4. Check statement reads against moved variables
            # Statements like `fmt.Println(x)` or `fn(x)`
            for v, state in list(variables.items()):
                canon = aliases.get(v, v)
                if variables.get(canon) == GhostState.MOVED:
                    # Check if v is referenced in code_part (excluding its own declaration)
                    if re.search(rf"\b{v}\b", code_part) and not re.search(rf"\b{v}\s*:=", code_part):
                        # Ensure it's not a pragma line
                        if not any(d == "move" and arg == v for d, arg in active_pragmas_for_next_stmt):
                            self.diagnostics.append(
                                Diagnostic(
                                    code="HYB-E1001",
                                    message=f"use of moved value '{v}'; ownership was previously transferred",
                                    line=idx,
                                    variable=v
                                )
                            )

            # 5. Process pending pragmas for this statement
            if active_pragmas_for_next_stmt:
                for directive, target in active_pragmas_for_next_stmt:
                    target_vars = [t.strip() for t in target.split(",") if t.strip()]

                    if directive == "move":
                        for tv in target_vars:
                            canon = aliases.get(tv, tv)
                            if canon in variables:
                                if variables[canon] == GhostState.MOVED:
                                    self.diagnostics.append(
                                        Diagnostic(
                                            code="HYB-E1001",
                                            message=f"double move: value '{tv}' was already moved",
                                            line=idx,
                                            variable=tv
                                        )
                                    )
                                else:
                                    variables[canon] = GhostState.MOVED
                            else:
                                variables[tv] = GhostState.MOVED

                    elif directive == "shared":
                        for tv in target_vars:
                            canon = aliases.get(tv, tv)
                            if canon in variables:
                                variables[canon] = GhostState.SHARED

                    elif directive == "borrow":
                        for tv in target_vars:
                            canon = aliases.get(tv, tv)
                            if canon in variables:
                                variables[canon] = GhostState.BORROWED

                active_pragmas_for_next_stmt = []

            # 6. Goroutine capture & concurrent race check: `go func() { ... x ... }()`
            if in_goroutine:
                for v, state in list(variables.items()):
                    if v in scope_stack[1].declared:  # Variable declared in outer function
                        if re.search(rf"\b{v}\b", code_part):
                            canon = aliases.get(v, v)
                            curr_state = variables.get(canon)
                            if curr_state not in (GhostState.SHARED, GhostState.MOVED):
                                self.diagnostics.append(
                                    Diagnostic(
                                        code="HYB-E3001",
                                        message=f"concurrent data race hazard: variable '{v}' captured across goroutine boundary without //hy:shared or //hy:move",
                                        line=idx,
                                        variable=v
                                    )
                                )

            # 7. Region escape check: `return ptr` or assignment escaping region scope
            if re.search(r"\breturn\b", code_part):
                for v, r in var_regions.items():
                    if re.search(rf"\b{v}\b", code_part):
                        self.diagnostics.append(
                            Diagnostic(
                                code="HYB-E4001",
                                message=f"region escape hazard: pointer '{v}' allocated in region '{r}' cannot escape enclosing region scope",
                                line=idx,
                                variable=v
                            )
                        )

        # Deduplicate diagnostics by (line, code, variable)
        unique_diags = []
        seen = set()
        for d in self.diagnostics:
            key = (d.line, d.code, d.variable)
            if key not in seen:
                seen.add(key)
                unique_diags.append(d)
        self.diagnostics = unique_diags

        return self.diagnostics


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python ghost_capability_gate.py <file.go>")
        sys.exit(1)

    gate = GhostCapabilityGate()
    with open(sys.argv[1], "r", encoding="utf-8") as f:
        diags = gate.check_program(f.read())

    if diags:
        print(f"FAILED with {len(diags)} diagnostics:")
        for d in diags:
            print(f"  [{d.code}] L{d.line}: {d.message}")
        sys.exit(1)
    else:
        print("OK (0 diagnostics): All ghost capabilities verified.")
        sys.exit(0)
