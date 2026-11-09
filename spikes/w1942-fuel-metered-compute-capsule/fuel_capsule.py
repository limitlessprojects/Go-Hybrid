"""W-1942 (#1942): Fuel-Metered Compute Capsules with a Closed Call Graph."""

from __future__ import annotations
from dataclasses import dataclass
from typing import List

FORBIDDEN_EFFECTS = {
    "ambient_go_call": "W1942_ERR_AMBIENT_GO_CALL",
    "callback_indirect": "W1942_ERR_INDIRECT_CALLBACK",
    "task_or_chan": "W1942_ERR_TASK_OR_CHAN_OP",
    "rust_ffi_bridge": "W1942_ERR_RUST_BRIDGE_CALL",
    "managed_hatch": "W1942_ERR_MANAGED_FORBIDDEN",
    "catch_fuel_reset": "W1942_ERR_FUEL_RESET_FORBIDDEN",
}


@dataclass(frozen=True)
class CapsuleStep:
    op: str  # "scalar_add" | "byte_scan" | "call_frame" | "return_frame" | forbidden effect key
    val: int = 1
    byte_len: int = 0


@dataclass(frozen=True)
class CapsuleResult:
    admitted: bool
    status: str  # "OK" | "BudgetExceeded" | "FrameDepthExceeded" | error code
    value: int | None
    fuel_remaining: int
    transitions_executed: int


def run_fuel_capsule(
    steps: List[CapsuleStep], initial_fuel: int, max_frame_depth: int = 8, max_byte_vec: int = 4096
) -> CapsuleResult:
    """Admit closed-call-graph steps and execute under a non-renewable logical fuel budget."""
    for st in steps:
        if st.op in FORBIDDEN_EFFECTS:
            return CapsuleResult(False, FORBIDDEN_EFFECTS[st.op], None, initial_fuel, 0)
        if st.op == "byte_scan" and st.byte_len > max_byte_vec:
            return CapsuleResult(False, "W1942_ERR_UNBOUNDED_BYTE_VEC", None, initial_fuel, 0)

    fuel = max(0, initial_fuel)
    acc = 0
    depth = 1
    executed = 0

    for st in steps:
        cost = 1 if st.op != "byte_scan" else max(1, st.byte_len)
        # Checked debit BEFORE transition executes
        if fuel < cost:
            return CapsuleResult(True, "BudgetExceeded", None, fuel, executed)
        fuel -= cost
        executed += 1

        if st.op == "scalar_add":
            acc += st.val
        elif st.op == "byte_scan":
            acc += st.byte_len
        elif st.op == "call_frame":
            depth += 1
            if depth > max_frame_depth:
                return CapsuleResult(True, "FrameDepthExceeded", None, fuel, executed)
        elif st.op == "return_frame":
            depth = max(1, depth - 1)

    return CapsuleResult(True, "OK", acc, fuel, executed)
