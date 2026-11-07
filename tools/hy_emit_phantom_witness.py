#!/usr/bin/env python3
"""W-004 (#174) Executable Spike: CPS-Scoped Phantom Witness Emitter (`tools/hy_emit_phantom_witness.py`).

Lowers Tier-0 Hybrid ownership, freeze, `may_share`, and loan invariants into Go phantom
capability tokens (`hyLive[R]`, `hyMoved`, `hyBorrowed[R]`, `hyLoan[R]`) with continuation-scoped
lexical shadowing and CPS join-point detection.
"""

from __future__ import annotations
from dataclasses import dataclass, field
import re
from typing import Dict, List, Set


@dataclass
class WitnessEmitResult:
    verdict: str  # "ACCEPT" or "REJECT"
    classification: str  # "LEXICAL_WITNESS", "CPS_TAIL_DUP", or "HY_CHECK_REQUIRED"
    emitted_go: str
    diagnostics: List[str] = field(default_factory=list)
    active_roots: Dict[str, str] = field(default_factory=dict)


DECL_OWN_RE = re.compile(r"^(?:let\s+(?:mut\s+|own\s+)?)?([A-Za-z_][A-Za-z0-9_]*)\s*:=\s*(.+)$")
PARAM_FN_RE = re.compile(r"^fn\s+[A-Za-z_][A-Za-z0-9_]*\s*\(([^)]*)\)")
CLOSURE_RE = re.compile(r"\bfn\s*\(([^)]*)\)\s*\{")
MOVE_RE = re.compile(r"\bmove\s+([A-Za-z_][A-Za-z0-9_]*)\b")
BORROW_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*:=\s*&([A-Za-z_][A-Za-z0-9_]*)")
DEFER_RE = re.compile(r"^defer\s+.*?\b([A-Za-z_][A-Za-z0-9_]*)\b")
USE_IDENT_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\b")
KEYWORDS = {
    "fn", "let", "mut", "own", "move", "if", "else", "for", "range", "return",
    "defer", "task", "import", "const", "true", "false", "print", "println", "use", "send", "work"
}


def emit_and_verify_witnesses(hy_source: str) -> WitnessEmitResult:
    """Emit Go phantom capability witnesses and simulate cmd/compile type-checking."""
    lines = hy_source.splitlines()
    var_to_root: Dict[str, str] = {}
    root_state: Dict[str, str] = {}  # root -> "hyLive", "hyMoved", "hyBorrowed"
    loaned_roots: Set[str] = set()
    deferred_roots: Set[str] = set()
    branch_moved_roots: Set[str] = set()
    diagnostics: List[str] = []
    go_lines: List[str] = [
        "type hyLive[R any] struct{}",
        "type hyMoved struct{}",
        "type hyBorrowed[R any] struct{}",
        "func hyRequireLive[R any](_ hyLive[R]) {}",
    ]

    in_loop = False
    loop_roots: Set[str] = set()
    in_if_branch = False
    in_closure = False
    classification = "LEXICAL_WITNESS"
    cps_indent = 0

    for line_no, raw in enumerate(lines, start=1):
        clean = raw.split("//")[0].strip()
        if not clean:
            continue

        m_fn = PARAM_FN_RE.match(clean)
        if m_fn:
            for p_tok in m_fn.group(1).split(","):
                parts = p_tok.strip().split()
                if parts:
                    pname = parts[0]
                    root = f"root_{pname}"
                    var_to_root[pname] = root
                    root_state[root] = "hyBorrowed"
                    go_lines.append(f"var hyCap_{pname} = hyBorrowed[{root}]{{}}")
            continue

        if clean.startswith("for ") or clean == "for {" or " range " in clean:
            in_loop = True
            loop_roots = {r for r, st in root_state.items() if st == "hyLive"}
            go_lines.append("for {")
            continue

        if clean.startswith("if "):
            in_if_branch = True
            go_lines.append(clean)
            continue

        if CLOSURE_RE.search(clean):
            in_closure = True
            for v, r in list(var_to_root.items()):
                if root_state.get(r) == "hyLive":
                    root_state[r] = "hyBorrowed"
            go_lines.append("// closure captures re-bound as hyBorrowed[R]")
            continue

        if clean == "}":
            if in_loop:
                for r in sorted(loop_roots):
                    go_lines.append(f"var _ hyLive[{r}] = hyCap_{r}")
                    if root_state.get(r) == "hyMoved":
                        diagnostics.append(
                            f"HYB-E3003 (A1c): loop-backedge witness 'var _ hyLive[{r}] = hyCap_{r}' failed at line {line_no} (hyMoved)"
                        )
                in_loop = False
            elif in_if_branch:
                in_if_branch = False
            elif in_closure:
                in_closure = False
            go_lines.append("}")
            continue

        if clean.startswith("defer "):
            for dvar in USE_IDENT_RE.findall(clean):
                if dvar in var_to_root:
                    deferred_roots.add(var_to_root[dvar])
                    go_lines.append(f"hyRequireLive(hyCap_{var_to_root[dvar]}) // defer registration")
            continue

        m_borrow = BORROW_RE.match(clean)
        if m_borrow:
            loan_var, src_var = m_borrow.group(1), m_borrow.group(2)
            root = var_to_root.get(src_var, f"root_{src_var}")
            var_to_root[loan_var] = root
            loaned_roots.add(root)
            go_lines.append(f"hyLoan_{loan_var} := hyCap_{root}")
            continue

        m_move = MOVE_RE.search(clean)
        if m_move:
            mvar = m_move.group(1)
            root = var_to_root.get(mvar)
            if root is None:
                root = f"root_{mvar}"
                var_to_root[mvar] = root
                root_state[root] = "hyLive"
            st = root_state.get(root, "hyLive")
            if st == "hyBorrowed":
                diagnostics.append(
                    f"HYB-E3006 (A5/#60): cannot move non-owned binding '{mvar}' (witness has type hyBorrowed[{root}]) at line {line_no}"
                )
            elif st == "hyMoved":
                diagnostics.append(
                    f"HYB-E3001 (A1b): double move of '{mvar}' (witness hyCap_{root} is hyMoved) at line {line_no}"
                )
            elif root in loaned_roots:
                diagnostics.append(
                    f"HYB-E3004 (A2/A3): cannot move '{mvar}' while active loan witness exists on {root} at line {line_no}"
                )
            else:
                if in_if_branch:
                    branch_moved_roots.add(root)
                    classification = "CPS_TAIL_DUP"
                else:
                    root_state[root] = "hyMoved"
                cps_indent += 1
                go_lines.append(f"{{ hyCap_{root} := hyMoved{{}} // CPS continuation scope after move {mvar}")
            continue

        m_decl = DECL_OWN_RE.match(clean)
        if m_decl:
            lhs, rhs = m_decl.group(1), m_decl.group(2).strip()
            if rhs in var_to_root:
                shared_root = var_to_root[rhs]
                var_to_root[lhs] = shared_root
                go_lines.append(f"hyCap_{lhs} := &hyCap_{shared_root} // #27 shared root phantom tag {shared_root}")
            else:
                root = f"root_{lhs}"
                var_to_root[lhs] = root
                root_state[root] = "hyLive"
                go_lines.append(f"var hyCap_{root} = hyLive[{root}]{{}}")
            continue

        for ident in USE_IDENT_RE.findall(clean):
            if ident in KEYWORDS or ident not in var_to_root:
                continue
            root = var_to_root[ident]
            go_lines.append(f"hyRequireLive(hyCap_{root})")
            if root in branch_moved_roots and not in_if_branch:
                diagnostics.append(
                    f"HYB-E3001 (M2 Join-Wall): post-if use of '{ident}' after conditional move requires CPS tail-duplication at line {line_no}"
                )
            elif root_state.get(root) == "hyMoved":
                diagnostics.append(
                    f"HYB-E3001 (A1b/#27): use of '{ident}' after move of root '{root}' (hyCap_{root} is hyMoved) at line {line_no}"
                )
            elif root in loaned_roots and ("=" in clean or "++" in clean):
                diagnostics.append(
                    f"HYB-E3004 (A2/A3): mutation of '{ident}' while loan witness is active on {root} at line {line_no}"
                )

    for droot in sorted(deferred_roots):
        if root_state.get(droot) == "hyMoved" or droot in branch_moved_roots:
            diagnostics.append(
                f"HYB-E3001 (defer-epilogue): deferred use of root '{droot}' fails function-exit hyRequireLive check"
            )

    for _ in range(cps_indent):
        go_lines.append("}")

    verdict = "REJECT" if diagnostics else "ACCEPT"
    return WitnessEmitResult(
        verdict=verdict,
        classification=classification,
        emitted_go="\n".join(go_lines),
        diagnostics=diagnostics,
        active_roots=dict(root_state),
    )
