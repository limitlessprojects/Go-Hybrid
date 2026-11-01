#!/usr/bin/env python3
"""W-1754 (#1754) Spike: Canonical CalleeRef IR + Zero-Transpile hy-vet Mode."""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

MUTABLE_TYPES = {"map[string]int", "[]byte", "[]int", "*State", "hy.Unique[Buffer]", "own[Buffer]"}


@dataclass(frozen=True)
class CalleeRef:
    receiver: Optional[str]
    base_symbol: str
    type_args: Tuple[str, ...] = ()

    @staticmethod
    def resolve(raw_expr: str) -> "CalleeRef":
        """Single canonical parser for parenthesized, qualified, and generic callee expressions."""
        s = raw_expr.strip()
        while s.startswith("(") and s.endswith(")"):
            inner = s[1:-1].strip()
            depth = 0
            balanced = True
            for i, ch in enumerate(inner):
                if ch == "(":
                    depth += 1
                elif ch == ")":
                    depth -= 1
                    if depth < 0:
                        balanced = False
                        break
            if balanced and depth == 0:
                s = inner
            else:
                break

        type_args: Tuple[str, ...] = ()
        if s.endswith("]") and "[" in s:
            bracket_start = s.find("[")
            raw_args = s[bracket_start + 1 : -1]
            type_args = tuple(a.strip() for a in raw_args.split(",") if a.strip())
            s = s[:bracket_start].strip()

        while s.startswith("(") and s.endswith(")"):
            s = s[1:-1].strip()

        if "." in s:
            recv, base = s.rsplit(".", 1)
            return CalleeRef(receiver=recv.strip("() "), base_symbol=base.strip(), type_args=type_args)
        return CalleeRef(receiver=None, base_symbol=s, type_args=type_args)


@dataclass
class FnSig:
    name: str
    type_params: Tuple[str, ...]
    param_types: Tuple[str, ...]
    consumes_first: bool = False


@dataclass
class CheckVerdict:
    verdict: str
    diagnostics: List[str] = field(default_factory=list)
    resolved_callees: List[CalleeRef] = field(default_factory=list)


def is_mutable_type(t_name: str) -> bool:
    return t_name in MUTABLE_TYPES or t_name.startswith(("map[", "[]", "*", "hy.Unique[", "own["))


def check_calleeref_program(
    fn_table: Dict[str, FnSig],
    bindings: Dict[str, str],
    calls: List[Dict[str, object]],
) -> CheckVerdict:
    diags: List[str] = []
    resolved: List[CalleeRef] = []
    moved: set = set()

    for call in calls:
        raw_callee = str(call["callee"])
        arg = str(call.get("arg", ""))
        is_task = bool(call.get("is_task", False))
        is_move = bool(call.get("is_move", False))
        is_managed = bool(call.get("is_managed", False))

        cref = CalleeRef.resolve(raw_callee)
        resolved.append(cref)

        if cref.receiver == "flate2" or cref.base_symbol.startswith("unbound_"):
            diags.append(f"HYB-E3005 (#1991): unbound foreign/crate call '{cref.base_symbol}' rejected")
            continue

        sig = fn_table.get(cref.base_symbol)
        if sig is None:
            diags.append(f"HYB-E3005: unresolved callee '{cref.base_symbol}' (fail-closed)")
            continue

        subst: Dict[str, str] = {}
        for tp, ta in zip(sig.type_params, cref.type_args):
            subst[tp] = ta

        arg_type = bindings.get(arg, "")
        if sig.param_types:
            expected_t = subst.get(sig.param_types[0], sig.param_types[0])
        else:
            expected_t = arg_type

        if arg in moved:
            diags.append(f"HYB-E3001: use of moved binding '{arg}' in call to '{cref.base_symbol}'")
            continue

        if is_task:
            if (is_mutable_type(arg_type) or is_mutable_type(expected_t)) and not is_move and not is_managed:
                diags.append(
                    f"HYB-E3002 (#1752): generic/spawn call '{cref.base_symbol}' with mutable '{expected_t}' requires move"
                )
            elif is_move:
                if arg_type.startswith(("hy.Weak[", "view[")):
                    diags.append(f"HYB-E3004: weak/view capability '{arg}' cannot cross task boundary")
                else:
                    moved.add(arg)
        elif sig.consumes_first or is_move:
            moved.add(arg)

    return CheckVerdict(
        verdict=("ACCEPT" if not diags else "REJECT"),
        diagnostics=diags,
        resolved_callees=resolved,
    )
