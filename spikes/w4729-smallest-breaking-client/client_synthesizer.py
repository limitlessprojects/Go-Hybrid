"""W-4729 (#4729): Smallest Downstream Breaking Client Synthesizer.

Compares two revisions of an ownership-typed API and synthesizes the minimal
downstream .hy program that passes under the old release and fails under the candidate.
"""

from __future__ import annotations
from collections import deque
from dataclasses import dataclass
from enum import Enum
from typing import Dict, List, Optional, Tuple


class Effect(str, Enum):
    BORROW = "borrow"    # Preserves live ownership
    CONSUME = "consume"  # Moves/consumes ownership (LIVE -> CONSUMED)


class ValueState(str, Enum):
    LIVE = "live"
    CONSUMED = "consumed"


@dataclass(frozen=True)
class Operation:
    name: str
    arg_type: str
    effect: Effect
    has_aliasing: bool = False


@dataclass
class APIPackage:
    name: str
    operations: Dict[str, Operation]


@dataclass
class SynthesisResult:
    has_breaking_change: bool
    status: str  # "WITNESS_FOUND", "COMPATIBLE", "UNKNOWN"
    witness_sequence: List[str]
    synthesized_hy_code: Optional[str] = None
    reason: Optional[str] = None


class BreakingClientSynthesizer:
    """Synthesizes minimal downstream .hy client programs demonstrating API ownership breaks."""

    def __init__(self, max_depth: int = 6):
        self.max_depth = max_depth

    def analyze_release_change(self, v1: APIPackage, v2: APIPackage) -> SynthesisResult:
        # Check for unsupported features (e.g. aliasing)
        for op in v1.operations.values():
            if op.has_aliasing:
                return SynthesisResult(
                    has_breaking_change=False,
                    status="UNKNOWN",
                    witness_sequence=[],
                    reason="Unsupported feature: aliasing detected in v1 API",
                )
        for op in v2.operations.values():
            if op.has_aliasing:
                return SynthesisResult(
                    has_breaking_change=False,
                    status="UNKNOWN",
                    witness_sequence=[],
                    reason="Unsupported feature: aliasing detected in v2 API",
                )

        # BFS state space exploration over operation sequences
        # State: (v1_state, v2_state, sequence_of_ops)
        # Search stops as soon as v1 is valid (accepts all ops) and v2 rejects (encounters CONSUMED).
        queue = deque([((ValueState.LIVE, ValueState.LIVE), [])])
        visited = set([((ValueState.LIVE, ValueState.LIVE), ())])

        ops = sorted(v1.operations.keys())

        while queue:
            (v1_st, v2_st), seq = queue.popleft()

            if len(seq) >= self.max_depth:
                continue

            for op_name in ops:
                if op_name not in v2.operations:
                    continue

                op1 = v1.operations[op_name]
                op2 = v2.operations[op_name]

                # In v1, can only execute if value is LIVE
                if v1_st != ValueState.LIVE:
                    continue  # Invalid sequence in v1

                # Step v1
                next_v1_st = ValueState.CONSUMED if op1.effect == Effect.CONSUME else ValueState.LIVE

                # Check v2
                if v2_st != ValueState.LIVE:
                    # In v2, this operation is rejected because the value is already CONSUMED!
                    # And v1 accepted this step and all previous steps!
                    witness = seq + [op_name]
                    code = self.generate_hy_code(v1.name, witness, v1, v2)
                    return SynthesisResult(
                        has_breaking_change=True,
                        status="WITNESS_FOUND",
                        witness_sequence=witness,
                        synthesized_hy_code=code,
                        reason=f"Old version accepted sequence {witness}, but candidate v2 rejects at step '{op_name}' (value already consumed)",
                    )

                next_v2_st = ValueState.CONSUMED if op2.effect == Effect.CONSUME else ValueState.LIVE
                new_seq = seq + [op_name]
                pair_key = ((next_v1_st, next_v2_st), tuple(new_seq))

                if pair_key not in visited:
                    visited.add(pair_key)
                    queue.append(((next_v1_st, next_v2_st), new_seq))

        # No breaking sequence found (either identical, compatible, or pure relaxation consume -> borrow)
        return SynthesisResult(
            has_breaking_change=False,
            status="COMPATIBLE",
            witness_sequence=[],
            reason="No breaking client sequence exists: candidate release preserves or expands permitted call sequences",
        )

    def generate_hy_code(
        self, pkg_name: str, witness: List[str], v1: APIPackage, v2: APIPackage
    ) -> str:
        """Generates a minimal downstream .hy program executing the witness sequence."""
        lines = [
            f"package main",
            "",
            f'import "{pkg_name}"',
            "",
            "// Downstream client witness synthesized by W-4729.",
            "// Compiles and passes with v1; rejected by v2 with HYB-E3001 (use of moved value).",
            "fn main() {",
            f"\tx := {pkg_name}.New()",
        ]

        for i, op in enumerate(witness):
            if i == len(witness) - 1:
                lines.append(f"\t// BREAK: Candidate v2 rejects call '{op}' because 'x' was moved in an earlier call")
            lines.append(f"\t{pkg_name}.{op}(x)")

        lines.append("}")
        return "\n".join(lines)


def replay_client(hy_code: str, v: APIPackage) -> Tuple[bool, Optional[str]]:
    """Simulates the Hybrid checker replaying the client code against a package version."""
    state = ValueState.LIVE
    for line in hy_code.splitlines():
        line = line.strip()
        if "(" in line and ")" in line and "New()" not in line and not line.startswith("//"):
            call_part = line.split("(")[0]
            if "." in call_part:
                op_name = call_part.split(".")[-1]
                if op_name in v.operations:
                    op = v.operations[op_name]
                    if state == ValueState.CONSUMED:
                        return False, f"HYB-E3001: use of moved value 'x' in call to '{op_name}'"
                    if op.effect == Effect.CONSUME:
                        state = ValueState.CONSUMED

    return True, None
