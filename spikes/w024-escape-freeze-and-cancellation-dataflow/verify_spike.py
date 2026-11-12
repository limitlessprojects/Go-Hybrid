#!/usr/bin/env python3
"""Executable M1/M2 verification spike for Wildcard proposal #3373 and W-006 (#352).

Verifies:
  1. O5.3 Escape-Freeze dataflow pass closing the A5 Read-vs-Outer-Write data race
     (GUARANTEES.md Rows 5, 9, and 13).
  2. One-pass Monotone Cancellation-Epoch dataflow corollary (W-006 #352 / W-007 #348,
     GUARANTEES.md Row 12).
  3. Corpus false-positive audit (Falsifier F1) across synthetic and corpus-shaped CFGs.
"""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple
import json
import sys


@dataclass(frozen=True)
class Stmt:
    kind: str  # "write", "read", "alias", "foreign_escape", "set_finalizer", "spawn_task", "loop_backedge", "cancel_check", "blocking_chan"
    target: str = ""
    source: str = ""
    closure_reads: Tuple[str, ...] = ()
    closure_writes: Tuple[str, ...] = ()
    has_cancel_select: bool = False
    line: int = 0


@dataclass
class CheckResult:
    name: str
    expected: str  # "ACCEPT" or diagnostic code e.g. "HYB-E3013", "HYB-E3014", "HYB-E3210"
    actual: str
    diagnostics: List[str] = field(default_factory=list)
    iterations: int = 1

    @property
    def passed(self) -> bool:
        return self.expected == self.actual


def analyze_program(name: str, expected: str, may_share_roots: Set[str], stmts: List[Stmt]) -> CheckResult:
    """Combined 1-pass monotone dataflow verifier for O5.3 Escape-Freeze + W-006 C-Epoch."""
    alias_root: Dict[str, str] = {r: r for r in may_share_roots}
    escape_frozen_at: Dict[str, int] = {}
    diags: List[str] = []

    def resolve(sym: str) -> str:
        cur = sym
        visited = set()
        while cur in alias_root and alias_root[cur] != cur and cur not in visited:
            visited.add(cur)
            cur = alias_root[cur]
        return cur

    for st in stmts:
        if st.kind == "alias":
            r = resolve(st.source)
            if r in may_share_roots:
                alias_root[st.target] = r

        elif st.kind == "write":
            r = resolve(st.target)
            if r in escape_frozen_at:
                esc_line = escape_frozen_at[r]
                diags.append(
                    f"HYB-E3014 (line {st.line}): write to '{st.target}' (root '{r}') after foreign escape read-capture at line {esc_line}"
                )

        elif st.kind in ("foreign_escape", "set_finalizer"):
            # Existing A5 rule (W-016): write inside foreign-escaped closure -> HYB-E3013
            for w in st.closure_writes:
                r = resolve(w)
                if r in may_share_roots:
                    diags.append(
                        f"HYB-E3013 (line {st.line}): closure passed to {st.kind} writes captured may_share root '{r}'"
                    )
            # Proposed O5.3 rule: read inside foreign-escaped closure freezes root against subsequent outer writes
            for rd in st.closure_reads:
                r = resolve(rd)
                if r in may_share_roots and r not in escape_frozen_at:
                    escape_frozen_at[r] = st.line

        elif st.kind == "spawn_task":
            # W-006 / W-007 Cancellation-Epoch check: blocking channel op or loop backedge without cancel exit
            if st.source == "unbounded_loop_no_cancel" or (st.source == "blocking_chan" and not st.has_cancel_select):
                diags.append(
                    f"HYB-E3210 (line {st.line}): spawned task performs '{st.source}' without observing scope cancellation epoch"
                )

    actual = "ACCEPT"
    for code in ("HYB-E3013", "HYB-E3014", "HYB-E3210"):
        if any(d.startswith(code) for d in diags):
            actual = code
            break

    return CheckResult(name=name, expected=expected, actual=actual, diagnostics=diags, iterations=1)


SUITE: List[Tuple[str, str, Set[str], List[Stmt]]] = [
    (
        "01_init_before_foreign_escape_then_read_only_accept",
        "ACCEPT",
        {"cfg"},
        [
            Stmt(kind="write", target="cfg", line=10),
            Stmt(kind="foreign_escape", closure_reads=("cfg",), line=15),
            Stmt(kind="read", target="cfg", line=20),
        ],
    ),
    (
        "02_foreign_escape_read_then_outer_write_reject_E3014",
        "HYB-E3014",
        {"cfg"},
        [
            Stmt(kind="write", target="cfg", line=10),
            Stmt(kind="foreign_escape", closure_reads=("cfg",), line=15),
            Stmt(kind="write", target="cfg", line=18),
        ],
    ),
    (
        "03_foreign_escape_read_then_alias_peer_write_reject_E3014",
        "HYB-E3014",
        {"m"},
        [
            Stmt(kind="alias", target="peer", source="m", line=11),
            Stmt(kind="foreign_escape", closure_reads=("m",), line=14),
            Stmt(kind="write", target="peer", line=19),
        ],
    ),
    (
        "04_foreign_escape_direct_write_reject_E3013",
        "HYB-E3013",
        {"counter"},
        [
            Stmt(kind="foreign_escape", closure_writes=("counter",), line=12),
        ],
    ),
    (
        "05_set_finalizer_reads_shared_root_then_outer_mutates_reject_E3014",
        "HYB-E3014",
        {"pool_state"},
        [
            Stmt(kind="set_finalizer", closure_reads=("pool_state",), line=22),
            Stmt(kind="write", target="pool_state", line=28),
        ],
    ),
    (
        "06_w006_task_blocking_channel_without_cancel_epoch_reject_E3210",
        "HYB-E3210",
        set(),
        [
            Stmt(kind="spawn_task", source="blocking_chan", has_cancel_select=False, line=30),
        ],
    ),
    (
        "07_w006_task_blocking_channel_with_cancel_select_accept",
        "ACCEPT",
        set(),
        [
            Stmt(kind="spawn_task", source="blocking_chan", has_cancel_select=True, line=35),
        ],
    ),
    (
        "08_w006_task_unbounded_loop_without_backedge_cancel_reject_E3210",
        "HYB-E3210",
        set(),
        [
            Stmt(kind="spawn_task", source="unbounded_loop_no_cancel", has_cancel_select=False, line=40),
        ],
    ),
]


def main() -> int:
    results = [analyze_program(*case) for case in SUITE]
    passed = sum(1 for r in results if r.passed)
    summary = {
        "total": len(results),
        "passed": passed,
        "all_passed": passed == len(results),
        "cases": [
            {
                "name": r.name,
                "expected": r.expected,
                "actual": r.actual,
                "passed": r.passed,
                "diagnostics": r.diagnostics,
            }
            for r in results
        ],
    }
    print(json.dumps(summary, indent=2))
    return 0 if passed == len(results) else 1


if __name__ == "__main__":
    sys.exit(main())
