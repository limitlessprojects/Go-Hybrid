#!/usr/bin/env python3
"""W-025 (#695) Spike: Explicit Desugared MIR (hy-mir) Lowering & 1:1 Stateless Emitter."""

from dataclasses import dataclass, field
from typing import Dict, List, Set, Tuple


@dataclass(frozen=True)
class MirInstr:
    op: str  # "AllocMut", "AllocConst", "AddrOf", "SlotLoanCopy", "SlotLoanStoreBack", "TaskSpawn", "Call"
    dst: str = ""
    src: str = ""
    extra: str = ""


@dataclass
class MirVerificationResult:
    verdict: str
    diagnostics: List[str] = field(default_factory=list)
    mir: List[MirInstr] = field(default_factory=list)
    emitted_go: List[str] = field(default_factory=list)
    bijection_ok: bool = True
    shadow_tables_used: int = 0


def desugar_ast_to_mir(
    ast_stmts: List[Dict[str, object]],
    ptr_param_fns: Set[str],
) -> List[MirInstr]:
    """Lowers implicit Go emission coercions into explicit hy-mir instructions before checking."""
    mir: List[MirInstr] = []
    deferred_store_backs: List[MirInstr] = []

    for st in ast_stmts:
        kind = str(st.get("kind", ""))
        if kind == "decl":
            is_const = bool(st.get("is_const", False))
            op = "AllocConst" if is_const else "AllocMut"
            mir.append(MirInstr(op=op, dst=str(st["name"]), src=str(st.get("init", "struct{}"))))

        elif kind == "slot_loan":
            loan_var = str(st["name"])
            base_map = str(st["map"])
            key = str(st.get("key", "k"))
            tmp = f"__hy_slot_{loan_var}"
            mir.append(MirInstr(op="SlotLoanCopy", dst=tmp, src=base_map, extra=key))
            mir.append(MirInstr(op="AddrOf", dst=loan_var, src=tmp))
            deferred_store_backs.append(MirInstr(op="SlotLoanStoreBack", dst=base_map, src=tmp, extra=key))

        elif kind == "call":
            fn_name = str(st["fn"])
            arg = str(st["arg"])
            if fn_name in ptr_param_fns:
                addr_tmp = f"__hy_autoaddr_{arg}"
                mir.append(MirInstr(op="AddrOf", dst=addr_tmp, src=arg, extra="auto_addr"))
                mir.append(MirInstr(op="Call", dst=fn_name, src=addr_tmp))
            else:
                mir.append(MirInstr(op="Call", dst=fn_name, src=arg))

        elif kind == "task_move":
            mir.append(MirInstr(op="TaskSpawn", dst=str(st["fn"]), src=str(st["target"]), extra="move"))

        elif kind == "early_return":
            for sb in reversed(deferred_store_backs):
                mir.append(sb)
            mir.append(MirInstr(op="Return"))

    for sb in reversed(deferred_store_backs):
        mir.append(sb)
    return mir


def emit_go_1to1(mir: List[MirInstr]) -> List[str]:
    """Stateless 1:1 Go printer from hy-mir with 0 shadow type/scope tables."""
    out: List[str] = []
    for ins in mir:
        if ins.op in ("AllocMut", "AllocConst"):
            out.append(f"var {ins.dst} = {ins.src}")
        elif ins.op == "SlotLoanCopy":
            out.append(f"var {ins.dst} = {ins.src}[{ins.extra}]")
        elif ins.op == "AddrOf":
            out.append(f"{ins.dst} := &{ins.src}")
        elif ins.op == "SlotLoanStoreBack":
            out.append(f"{ins.src}_back -> {ins.dst}[{ins.extra}] = {ins.src}")
        elif ins.op == "Call":
            out.append(f"{ins.dst}({ins.src})")
        elif ins.op == "TaskSpawn":
            out.append(f"go {ins.dst}({ins.src})")
        elif ins.op == "Return":
            out.append("return")
    return out


def check_and_emit_mir(
    ast_stmts: List[Dict[str, object]],
    ptr_param_fns: Set[str],
) -> MirVerificationResult:
    mir = desugar_ast_to_mir(ast_stmts, ptr_param_fns)
    diags: List[str] = []
    const_bindings: Set[str] = set()
    moved_bindings: Set[str] = set()
    active_slot_loans: Dict[str, str] = {}

    for ins in mir:
        if ins.op == "AllocConst":
            const_bindings.add(ins.dst)
        elif ins.op == "SlotLoanCopy":
            active_slot_loans[ins.dst] = ins.src
        elif ins.op == "AddrOf":
            base = active_slot_loans.get(ins.src, ins.src)
            if base in const_bindings:
                diags.append(f"HYB-E3004 (T22/#188): cannot take address (&) of const binding '{base}'")
            if base in moved_bindings:
                diags.append(f"HYB-E3001: cannot auto-address or borrow moved binding '{base}'")
        elif ins.op == "TaskSpawn" and ins.extra == "move":
            if ins.src in moved_bindings:
                diags.append(f"HYB-E3001: double move of '{ins.src}'")
            if ins.src in active_slot_loans.values():
                diags.append(f"HYB-E3104 (#546): cannot move '{ins.src}' while slot-loan store-back is pending")
            moved_bindings.add(ins.src)
        elif ins.op == "SlotLoanStoreBack":
            if ins.dst in moved_bindings:
                diags.append(f"HYB-E3104 (#546): slot-loan store-back to moved map '{ins.dst}' races with task")

    emitted = emit_go_1to1(mir)
    bijection_ok = len(emitted) == len(mir)
    verdict = "ACCEPT" if not diags and bijection_ok else "REJECT"
    return MirVerificationResult(
        verdict=verdict,
        diagnostics=diags,
        mir=mir,
        emitted_go=emitted,
        bijection_ok=bijection_ok,
        shadow_tables_used=0,
    )
