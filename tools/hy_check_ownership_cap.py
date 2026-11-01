#!/usr/bin/env python3
"""W-002 (#279) Executable Spike: Type-Carried Ownership Capabilities with Local Inference."""

from __future__ import annotations
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple


@dataclass(frozen=True)
class CapType:
    mode: str  # "own", "share", "borrow", "moved"
    base_type: str = "[]byte"
    region_depth: int = 0
    source_binding: Optional[str] = None
    inferred: bool = True


@dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str
    binding: str


@dataclass
class CheckResult:
    accepted: bool
    diagnostics: List[Diagnostic] = field(default_factory=list)
    final_env: Dict[str, CapType] = field(default_factory=dict)
    deleted_tables_used: int = 0


BUILTIN_RETURN_CAP: Dict[str, Tuple[str, str]] = {
    "make_slice": ("own", "[]byte"),
    "chunk_of": ("own", "[]byte"),
    "identity_slice": ("borrow_arg0", "[]byte"),
    "subslice_view": ("borrow_arg0", "[]byte"),
}


def check_program(statements: List[Dict[str, object]]) -> CheckResult:
    env: Dict[str, CapType] = {}
    diags: List[Diagnostic] = []

    def eval_block(stmts: List[Dict[str, object]], cur_env: Dict[str, CapType], depth: int, in_loop: bool = False) -> Dict[str, CapType]:
        local_env = dict(cur_env)
        outer_names = set(cur_env.keys())
        passes = 2 if in_loop else 1
        for pass_idx in range(passes):
            step_env = dict(local_env)
            shadowed_names = set()
            for st in stmts:
                op = str(st.get("op", ""))
                if op == "let":
                    name = str(st["name"])
                    if depth > 0 and name in outer_names:
                        shadowed_names.add(name)
                    rhs_kind = str(st.get("rhs_kind", "alloc"))
                    rhs_arg = str(st.get("rhs_arg", "")) if st.get("rhs_arg") is not None else ""
                    explicit = str(st["explicit_mode"]) if st.get("explicit_mode") else None
                    if rhs_kind == "alloc":
                        step_env[name] = CapType(mode=explicit or "own", region_depth=depth)
                    elif rhs_kind == "call" and ":" in rhs_arg:
                        fn_name, arg0 = rhs_arg.split(":", 1)
                        sig_mode, base_t = BUILTIN_RETURN_CAP.get(fn_name, ("own", "[]byte"))
                        src_cap = step_env.get(arg0)
                        if src_cap is None or src_cap.mode == "moved":
                            if pass_idx == 0 or not diags:
                                diags.append(Diagnostic("E3001", f"use of moved capability '{arg0}'", arg0))
                            step_env[name] = CapType(mode="moved", region_depth=depth)
                        elif sig_mode == "own":
                            step_env[name] = CapType(mode=explicit or "own", base_type=base_t, region_depth=depth)
                        else:
                            step_env[name] = CapType(mode=explicit or "borrow", base_type=src_cap.base_type, region_depth=depth, source_binding=arg0)
                    elif rhs_kind == "borrow_var":
                        src_cap = step_env.get(rhs_arg)
                        step_env[name] = CapType(mode="borrow", base_type=src_cap.base_type if src_cap else "[]byte", region_depth=depth, source_binding=rhs_arg)
                    elif rhs_kind == "share_var":
                        src_cap = step_env.get(rhs_arg)
                        if src_cap:
                            step_env[rhs_arg] = CapType(mode="share", base_type=src_cap.base_type, region_depth=src_cap.region_depth)
                        step_env[name] = CapType(mode="share", region_depth=depth, source_binding=rhs_arg)
                elif op == "move_sink":
                    target = str(st["target"])
                    cap = step_env.get(target)
                    if cap is None or cap.mode == "moved":
                        if not any(d.code == "E3001" and d.binding == target for d in diags):
                            diags.append(Diagnostic("E3001", f"use of moved capability '{target}'", target))
                    elif cap.mode != "own":
                        code = "E3001" if cap.source_binding and in_loop else "E3101"
                        if not any(d.binding == target for d in diags):
                            diags.append(Diagnostic(code, f"cannot consume non-owned '{target}'", target))
                        if cap.source_binding and cap.source_binding in step_env:
                            step_env[cap.source_binding] = CapType(mode="moved", base_type=cap.base_type)
                    else:
                        step_env[target] = CapType(mode="moved", base_type=cap.base_type, region_depth=cap.region_depth)
                elif op == "mutate":
                    target = str(st["target"])
                    cap = step_env.get(target)
                    if cap is None or cap.mode == "moved":
                        diags.append(Diagnostic("E3001", f"mutation of moved '{target}'", target))
                    elif cap.mode != "own" or any(k != target and c.mode in ("borrow", "share") and c.source_binding == target for k, c in step_env.items()):
                        diags.append(Diagnostic("E3003", f"cannot mutate '{target}' while borrowed or shared", target))
                elif op == "assign_outer":
                    inner_var = str(st["inner"])
                    inner_cap = step_env.get(inner_var)
                    if inner_cap and inner_cap.mode == "borrow" and inner_cap.region_depth >= depth:
                        diags.append(Diagnostic("E3004", f"borrowed capability '{inner_var}' escapes region", inner_var))
                elif op == "closure_spawn":
                    captures = list(st.get("captures", []))
                    is_move = bool(st.get("is_move", False))
                    mutates = list(st.get("mutates", []))
                    for cv in captures:
                        c = step_env.get(cv)
                        if c is None or c.mode == "moved":
                            diags.append(Diagnostic("E3001", f"closure captures moved '{cv}'", cv))
                        elif is_move:
                            if c.mode != "own":
                                diags.append(Diagnostic("E3101", f"move closure requires own[T] for '{cv}'", cv))
                            else:
                                step_env[cv] = CapType(mode="moved", base_type=c.base_type, region_depth=c.region_depth)
                        else:
                            if c.mode == "own":
                                diags.append(Diagnostic("E3006", f"escaping closure captures own '{cv}' without move/share", cv))
                            elif c.mode == "borrow":
                                diags.append(Diagnostic("E3004", f"escaping closure captures borrow '{cv}'", cv))
                            elif c.mode == "share" and cv in mutates:
                                diags.append(Diagnostic("E3003", f"closure cannot mutate shared '{cv}'", cv))
                elif op == "block":
                    step_env = eval_block(list(st.get("body", [])), step_env, depth + 1, in_loop=False)
                elif op == "loop":
                    step_env = eval_block(list(st.get("body", [])), step_env, depth + 1, in_loop=True)
            for k in outer_names - shadowed_names:
                if k in step_env:
                    local_env[k] = step_env[k]
            if depth == 0:
                local_env = step_env
        return local_env

    final_env = eval_block(statements, env, depth=0, in_loop=False)
    return CheckResult(accepted=(len(diags) == 0), diagnostics=diags, final_env=final_env, deleted_tables_used=0)
