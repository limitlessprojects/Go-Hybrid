"""W-4729: Directional ownership-breaking release witness synthesizer and replay checker.

Given two revisions of a checked API (old vs. candidate) with explicit ownership
effects (`borrow`, `consume`, `unknown`), this module:
1. Synthesizes the shortest straight-line downstream `.hy` client that compiles
   against `old` and fails against `candidate` via breadth-first search over
   `(old_state, new_state)` pairs.
2. Refuses to guess on unsupported aliasing/escaping effects (`unknown`).
3. Ignores alpha-equivalent local variable renames in unchanged APIs (`no_witness`).
4. Emits a `.hy` client and verifies replay on a pinned ownership checker against
   both revisions before reporting a breaking change.
"""

from __future__ import annotations

from collections import deque
from dataclasses import dataclass, field
from enum import Enum
from itertools import product
import re
from typing import Dict, List, Optional, Tuple


class OpEffect(str, Enum):
    BORROW = "borrow"
    CONSUME = "consume"
    UNKNOWN = "unknown"  # aliasing, returned loan, callback, or multi-value escape


class ValueState(str, Enum):
    LIVE = "live"
    CONSUMED = "consumed"


class WitnessStatus(str, Enum):
    BREAKING_CHANGE = "breaking_change"
    NO_WITNESS = "no_witness"
    UNKNOWN = "unknown"
    REPLAY_MISMATCH = "replay_mismatch"


@dataclass(frozen=True)
class ApiOp:
    name: str
    effect: OpEffect
    param_name: str = "x"
    body_local_names: Tuple[str, ...] = ()


@dataclass(frozen=True)
class ApiRevision:
    version: str
    toolchain: str
    ops: Dict[str, ApiOp]

    def public_contract(self) -> Dict[str, OpEffect]:
        """Public ownership contract ignores internal body local variable names."""
        return {name: op.effect for name, op in sorted(self.ops.items())}


@dataclass(frozen=True)
class CheckDiagnostic:
    code: str
    message: str
    call_index: int
    op_name: str


@dataclass(frozen=True)
class CheckResult:
    passed: bool
    diagnostic: Optional[CheckDiagnostic] = None


@dataclass(frozen=True)
class ReleaseDiffReport:
    status: WitnessStatus
    old_version: str
    candidate_version: str
    toolchain: str
    witness_calls: Tuple[str, ...] = ()
    hy_client_source: str = ""
    old_check: Optional[CheckResult] = None
    candidate_check: Optional[CheckResult] = None
    reason: str = ""


_CALL_RE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*;?\s*$")


def step_state(state: ValueState, effect: OpEffect) -> Optional[ValueState]:
    """Advance single-owned-value state under an explicit ownership effect.

    Returns None when the operation is rejected (use-after-consume).
    """
    if state == ValueState.CONSUMED:
        return None
    if effect == OpEffect.BORROW:
        return ValueState.LIVE
    if effect == OpEffect.CONSUME:
        return ValueState.CONSUMED
    raise ValueError(f"Cannot deterministically step unknown effect: {effect}")


def emit_hy_client(calls: Tuple[str, ...], package_name: str = "client") -> str:
    """Emit a deterministic straight-line `.hy` downstream client program."""
    lines = [
        f"package {package_name}",
        "",
        "import api",
        "",
        "func Exercise(owned x api.Resource) {",
    ]
    for op_name in calls:
        lines.append(f"    api.{op_name}(x)")
    lines.append("}")
    lines.append("")
    return "\n".join(lines)


def parse_hy_client(source: str) -> List[Tuple[str, str]]:
    """Extract straight-line `api.<Op>(<var>)` calls from a generated `.hy` client."""
    calls: List[Tuple[str, str]] = []
    in_func = False
    for raw_line in source.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("//") or line.startswith("package ") or line.startswith("import "):
            continue
        if line.startswith("func ") and line.endswith("{"):
            in_func = True
            continue
        if line == "}":
            in_func = False
            continue
        if not in_func:
            continue
        if line.startswith("api."):
            line = line[4:]
        m = _CALL_RE.match(line)
        if not m:
            raise ValueError(f"Unsupported statement in pilot .hy client: {raw_line!r}")
        calls.append((m.group(1), m.group(2)))
    return calls


def check_hy_client(source: str, revision: ApiRevision) -> CheckResult:
    """Deterministic pinned ownership checker for straight-line single-value `.hy` clients."""
    calls = parse_hy_client(source)
    states: Dict[str, ValueState] = {}
    last_consumer: Dict[str, str] = {}

    for idx, (op_name, var_name) in enumerate(calls):
        op = revision.ops.get(op_name)
        if op is None:
            return CheckResult(
                passed=False,
                diagnostic=CheckDiagnostic(
                    code="HYB-API001",
                    message=f"unknown API operation '{op_name}' in {revision.version}",
                    call_index=idx,
                    op_name=op_name,
                ),
            )
        if op.effect == OpEffect.UNKNOWN:
            return CheckResult(
                passed=False,
                diagnostic=CheckDiagnostic(
                    code="HYB-REL001",
                    message=f"unsupported aliasing/escaping effect on '{op_name}'",
                    call_index=idx,
                    op_name=op_name,
                ),
            )

        cur_state = states.get(var_name, ValueState.LIVE)
        nxt = step_state(cur_state, op.effect)
        if nxt is None:
            consumed_by = last_consumer.get(var_name, "previous call")
            return CheckResult(
                passed=False,
                diagnostic=CheckDiagnostic(
                    code="HYB-OWN001",
                    message=f"use of consumed value '{var_name}' in '{op_name}({var_name})' (consumed by '{consumed_by}')",
                    call_index=idx,
                    op_name=op_name,
                ),
            )
        states[var_name] = nxt
        if op.effect == OpEffect.CONSUME:
            last_consumer[var_name] = f"{op_name}({var_name})"

    return CheckResult(passed=True, diagnostic=None)


def find_shortest_breaking_witness(
    old: ApiRevision,
    candidate: ApiRevision,
) -> ReleaseDiffReport:
    """Synthesize the shortest downstream `.hy` client that passes on `old` and fails on `candidate`."""
    if old.toolchain != candidate.toolchain:
        return ReleaseDiffReport(
            status=WitnessStatus.UNKNOWN,
            old_version=old.version,
            candidate_version=candidate.version,
            toolchain=f"{old.toolchain} != {candidate.toolchain}",
            reason="Toolchains must be pinned identically for both builds.",
        )

    # Reject unsupported aliasing / escaping ops immediately rather than guessing.
    for rev in (old, candidate):
        for op_name, op in sorted(rev.ops.items()):
            if op.effect == OpEffect.UNKNOWN:
                return ReleaseDiffReport(
                    status=WitnessStatus.UNKNOWN,
                    old_version=old.version,
                    candidate_version=candidate.version,
                    toolchain=old.toolchain,
                    reason=f"Unsupported aliasing or escaping ownership effect in {rev.version}.{op_name}.",
                )

    if old.public_contract() == candidate.public_contract():
        return ReleaseDiffReport(
            status=WitnessStatus.NO_WITNESS,
            old_version=old.version,
            candidate_version=candidate.version,
            toolchain=old.toolchain,
            reason="Public ownership contracts are identical (local renames ignored).",
        )

    op_names = sorted(set(old.ops.keys()) & set(candidate.ops.keys()))
    start = (ValueState.LIVE, ValueState.LIVE)
    queue: deque[Tuple[ValueState, ValueState, Tuple[str, ...]]] = deque([(start[0], start[1], ())])
    visited = {start}

    while queue:
        old_s, new_s, path = queue.popleft()
        for op_name in op_names:
            old_next = step_state(old_s, old.ops[op_name].effect)
            if old_next is None:
                continue  # Must remain legal in the old release.

            new_next = step_state(new_s, candidate.ops[op_name].effect)
            next_path = path + (op_name,)
            if new_next is None:
                # Found shortest witness accepted by old and rejected by candidate.
                hy_src = emit_hy_client(next_path)
                old_chk = check_hy_client(hy_src, old)
                new_chk = check_hy_client(hy_src, candidate)
                predicted_idx = len(next_path) - 1
                if (
                    old_chk.passed
                    and not new_chk.passed
                    and new_chk.diagnostic is not None
                    and new_chk.diagnostic.call_index == predicted_idx
                    and new_chk.diagnostic.op_name == op_name
                ):
                    return ReleaseDiffReport(
                        status=WitnessStatus.BREAKING_CHANGE,
                        old_version=old.version,
                        candidate_version=candidate.version,
                        toolchain=old.toolchain,
                        witness_calls=next_path,
                        hy_client_source=hy_src,
                        old_check=old_chk,
                        candidate_check=new_chk,
                        reason=f"Old passes; candidate fails at call #{predicted_idx} ({op_name}).",
                    )
                return ReleaseDiffReport(
                    status=WitnessStatus.REPLAY_MISMATCH,
                    old_version=old.version,
                    candidate_version=candidate.version,
                    toolchain=old.toolchain,
                    witness_calls=next_path,
                    hy_client_source=hy_src,
                    old_check=old_chk,
                    candidate_check=new_chk,
                    reason="Generated witness failed pinned checker replay.",
                )

            pair = (old_next, new_next)
            if pair not in visited:
                visited.add(pair)
                queue.append((old_next, new_next, next_path))

    return ReleaseDiffReport(
        status=WitnessStatus.NO_WITNESS,
        old_version=old.version,
        candidate_version=candidate.version,
        toolchain=old.toolchain,
        reason="Candidate is backwards-compatible (every old-accepted client still compiles).",
    )


def enumerate_breaking_sequences(
    old: ApiRevision,
    candidate: ApiRevision,
    max_len: int = 6,
) -> Tuple[int, int]:
    """Exhaustively enumerate all sequences of length 0..max_len.

    Returns (total_sequences, old_accepted_new_rejected_count).
    """
    op_names = sorted(set(old.ops.keys()) & set(candidate.ops.keys()))
    total = 0
    breaking = 0
    for length in range(0, max_len + 1):
        for seq in product(op_names, repeat=length):
            total += 1
            src = emit_hy_client(seq)
            if check_hy_client(src, old).passed and not check_hy_client(src, candidate).passed:
                breaking += 1
    return total, breaking
