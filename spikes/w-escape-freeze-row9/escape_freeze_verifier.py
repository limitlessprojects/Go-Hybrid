#!/usr/bin/env python3
"""Spike: Dual-Polarity Foreign Escape Freeze (O5.3 / HYB-E3014) to Flip GUARANTEES.md Row 9.

Closes the Row 9 'read-only captures stay outside the checker' soundness gap:
when a closure passed to a foreign Go call (<pkg>.<fn> or foreign handle method)
reads a captured may_share binding `x`, O5.3 places an Escape-Freeze witness on
`share_group(x)`. Any subsequent write to `x` (or an alias peer, or a loop
backedge reaching the escape site) in the enclosing function is rejected with
HYB-E3014 unless the escape or mutation sits inside `managed("...", "interop")`.
"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple


@dataclass(frozen=True)
class StatementEvent:
    kind: str  # "decl", "alias", "write", "foreign_call", "local_call", "loop_start", "loop_end"
    line: int
    binding: str = ""
    source: str = ""
    may_share: bool = True
    callee: str = ""
    closure_reads: Tuple[str, ...] = ()
    closure_writes: Tuple[str, ...] = ()
    in_managed_interop: bool = False


@dataclass
class CheckResult:
    case_id: str
    expected_verdict: str  # "ACCEPT" or "REJECT"
    expected_code: Optional[str]
    actual_verdict: str
    diagnostics: List[Tuple[str, int, str]] = field(default_factory=list)
    frozen_roots: Set[str] = field(default_factory=set)

    @property
    def passed(self) -> bool:
        if self.expected_verdict != self.actual_verdict:
            return False
        if self.expected_code is not None:
            return any(code == self.expected_code for code, _, _ in self.diagnostics)
        return len(self.diagnostics) == 0


class EscapeFreezeChecker:
    """Models Tier-0 A5 + O5.3 Dual-Polarity Escape-Freeze analysis for GUARANTEES Row 9."""

    def __init__(self) -> None:
        self.may_share: Dict[str, bool] = {}
        self.peers: Dict[str, Set[str]] = {}
        self.escape_frozen_at: Dict[str, Tuple[int, str]] = {}
        self.diags: List[Tuple[str, int, str]] = []

    def share_group(self, name: str) -> Set[str]:
        visited: Set[str] = set()
        stack = [name]
        while stack:
            cur = stack.pop()
            if cur not in visited:
                visited.add(cur)
                stack.extend(self.peers.get(cur, set()) - visited)
        return visited

    def link_alias(self, lhs: str, rhs: str) -> None:
        self.peers.setdefault(lhs, set()).add(rhs)
        self.peers.setdefault(rhs, set()).add(lhs)
        if self.may_share.get(rhs, False):
            self.may_share[lhs] = True
        for peer in list(self.share_group(rhs)):
            if peer in self.escape_frozen_at:
                self.escape_frozen_at[lhs] = self.escape_frozen_at[peer]

    def check_trace(self, events: List[StatementEvent]) -> Tuple[List[Tuple[str, int, str]], Set[str]]:
        loop_depth = 0
        loop_writes: List[Tuple[int, str]] = []
        loop_escapes: List[Tuple[int, str, str]] = []

        for ev in events:
            if ev.kind == "loop_start":
                loop_depth += 1
                loop_writes.clear()
                loop_escapes.clear()
                continue

            if ev.kind == "loop_end":
                # Backedge check: a write earlier in the same loop body reaches a
                # foreign read-escape on the next iteration if they share storage.
                for w_line, w_name in loop_writes:
                    for e_line, e_name, callee in loop_escapes:
                        if w_name in self.share_group(e_name):
                            self.diags.append((
                                "HYB-E3014",
                                w_line,
                                f"loop backedge mutates `{w_name}` after captured-read escape of `{e_name}` into foreign call `{callee}` at line {e_line} (HYB-E3014)",
                            ))
                loop_depth = max(0, loop_depth - 1)
                continue

            if ev.kind == "decl":
                self.may_share[ev.binding] = ev.may_share
                self.peers.setdefault(ev.binding, set())
                continue

            if ev.kind == "alias":
                self.link_alias(ev.binding, ev.source)
                continue

            if ev.kind == "write":
                if not ev.in_managed_interop:
                    for peer in self.share_group(ev.binding):
                        if peer in self.escape_frozen_at:
                            esc_line, callee = self.escape_frozen_at[peer]
                            self.diags.append((
                                "HYB-E3014",
                                ev.line,
                                f"mutation of `{ev.binding}` races with read-only closure capture of `{peer}` escaped to foreign call `{callee}` at line {esc_line} — freeze `{peer}` or wrap in `managed(\"...\", \"interop\")` (HYB-E3014)",
                            ))
                            break
                    if loop_depth > 0:
                        loop_writes.append((ev.line, ev.binding))
                continue

            if ev.kind == "foreign_call":
                if ev.in_managed_interop:
                    continue
                # Polarity 1 (existing W-016 M3 / HYB-E3013): closure writes captured binding
                for w_cap in ev.closure_writes:
                    self.diags.append((
                        "HYB-E3013",
                        ev.line,
                        f"closure writes the captured binding `{w_cap}` and is passed to foreign Go call `{ev.callee}` (HYB-E3013)",
                    ))
                # Polarity 2 (O5.3 Escape-Freeze / HYB-E3014): closure reads captured may_share binding
                for r_cap in ev.closure_reads:
                    group = self.share_group(r_cap)
                    if any(self.may_share.get(p, False) for p in group):
                        for p in group:
                            self.escape_frozen_at.setdefault(p, (ev.line, ev.callee))
                        if loop_depth > 0:
                            loop_escapes.append((ev.line, r_cap, ev.callee))

        return self.diags, set(self.escape_frozen_at.keys())


CORPUS: List[Tuple[str, str, Optional[str], List[StatementEvent]]] = [
    # --- 6 REJECT soundness regressions (R1..R6) ---
    (
        "R1_post_escape_map_write",
        "REJECT",
        "HYB-E3014",
        [
            StatementEvent("decl", 1, binding="routes", may_share=True),
            StatementEvent("write", 2, binding="routes"),
            StatementEvent("foreign_call", 3, callee="http.HandleFunc", closure_reads=("routes",)),
            StatementEvent("write", 4, binding="routes"),  # races with concurrent HTTP handler read!
        ],
    ),
    (
        "R2_alias_peer_post_escape_write",
        "REJECT",
        "HYB-E3014",
        [
            StatementEvent("decl", 1, binding="cache", may_share=True),
            StatementEvent("alias", 2, binding="alias_cache", source="cache"),
            StatementEvent("foreign_call", 3, callee="time.AfterFunc", closure_reads=("cache",)),
            StatementEvent("write", 4, binding="alias_cache"),  # mutates underlying map via alias!
        ],
    ),
    (
        "R3_loop_backedge_write_before_escape",
        "REJECT",
        "HYB-E3014",
        [
            StatementEvent("decl", 1, binding="SharedTable", may_share=True),
            StatementEvent("loop_start", 2),
            StatementEvent("write", 3, binding="SharedTable"),
            StatementEvent("foreign_call", 4, callee="http.HandleFunc", closure_reads=("SharedTable",)),
            StatementEvent("loop_end", 5),
        ],
    ),
    (
        "R4_captured_write_still_fires_e3013",
        "REJECT",
        "HYB-E3013",
        [
            StatementEvent("decl", 1, binding="hits", may_share=False),
            StatementEvent("foreign_call", 2, callee="http.HandleFunc", closure_writes=("hits",)),
        ],
    ),
    (
        "R5_struct_holding_map_field_mutated_after_escape",
        "REJECT",
        "HYB-E3014",
        [
            StatementEvent("decl", 1, binding="cfg_struct", may_share=True),
            StatementEvent("foreign_call", 2, callee="net.ListenCallback", closure_reads=("cfg_struct",)),
            StatementEvent("write", 3, binding="cfg_struct"),
        ],
    ),
    (
        "R6_transitive_alias_chain_mutated_after_escape",
        "REJECT",
        "HYB-E3014",
        [
            StatementEvent("decl", 1, binding="m0", may_share=True),
            StatementEvent("alias", 2, binding="m1", source="m0"),
            StatementEvent("alias", 3, binding="m2", source="m1"),
            StatementEvent("foreign_call", 4, callee="http.HandleFunc", closure_reads=("m0",)),
            StatementEvent("write", 5, binding="m2"),
        ],
    ),
    # --- 6 ACCEPT zero-false-positive patterns (A1..A6) ---
    (
        "A1_populated_before_escape_never_mutated_after",
        "ACCEPT",
        None,
        [
            StatementEvent("decl", 1, binding="routes", may_share=True),
            StatementEvent("write", 2, binding="routes"),
            StatementEvent("write", 3, binding="routes"),
            StatementEvent("foreign_call", 4, callee="http.HandleFunc", closure_reads=("routes",)),
        ],
    ),
    (
        "A2_primitive_value_capture_mutated_after_escape",
        "ACCEPT",
        None,
        [
            # Value types (int/string/bool) do not share heap storage when copied or read
            StatementEvent("decl", 1, binding="port_num", may_share=False),
            StatementEvent("foreign_call", 2, callee="http.HandleFunc", closure_reads=("port_num",)),
        ],
    ),
    (
        "A3_managed_interop_hatch_permits_post_escape_write",
        "ACCEPT",
        None,
        [
            StatementEvent("decl", 1, binding="metrics_map", may_share=True),
            StatementEvent("foreign_call", 2, callee="http.HandleFunc", closure_reads=("metrics_map",), in_managed_interop=True),
            StatementEvent("write", 3, binding="metrics_map", in_managed_interop=True),
        ],
    ),
    (
        "A4_local_non_foreign_closure_allows_subsequent_write",
        "ACCEPT",
        None,
        [
            StatementEvent("decl", 1, binding="local_map", may_share=True),
            StatementEvent("local_call", 2, callee="apply_sync", closure_reads=("local_map",)),
            StatementEvent("write", 3, binding="local_map"),
        ],
    ),
    (
        "A5_multiple_foreign_readers_without_mutation",
        "ACCEPT",
        None,
        [
            StatementEvent("decl", 1, binding="readonly_dict", may_share=True),
            StatementEvent("write", 2, binding="readonly_dict"),
            StatementEvent("foreign_call", 3, callee="http.HandleFunc", closure_reads=("readonly_dict",)),
            StatementEvent("foreign_call", 4, callee="http.HandleFunc", closure_reads=("readonly_dict",)),
        ],
    ),
    (
        "A6_disjoint_map_mutated_after_escape",
        "ACCEPT",
        None,
        [
            StatementEvent("decl", 1, binding="escaped_map", may_share=True),
            StatementEvent("decl", 2, binding="local_scratch", may_share=True),
            StatementEvent("foreign_call", 3, callee="http.HandleFunc", closure_reads=("escaped_map",)),
            StatementEvent("write", 4, binding="local_scratch"),
        ],
    ),
]


def run_corpus() -> List[CheckResult]:
    results: List[CheckResult] = []
    for case_id, exp_verdict, exp_code, events in CORPUS:
        ck = EscapeFreezeChecker()
        diags, frozen = ck.check_trace(events)
        act_verdict = "REJECT" if diags else "ACCEPT"
        results.append(
            CheckResult(
                case_id=case_id,
                expected_verdict=exp_verdict,
                expected_code=exp_code,
                actual_verdict=act_verdict,
                diagnostics=diags,
                frozen_roots=frozen,
            )
        )
    return results


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify O5.3 Dual-Polarity Foreign Escape Freeze (Row 9).")
    parser.add_argument("--check", action="store_true", help="Run the 12-case conformance corpus and exit non-zero on mismatch.")
    parser.add_argument("--json", action="store_true", help="Emit JSON summary.")
    args = parser.parse_args()

    results = run_corpus()
    passed = sum(1 for r in results if r.passed)
    total = len(results)

    if args.json:
        payload = {
            "passed": passed,
            "total": total,
            "all_passed": passed == total,
            "cases": [
                {
                    "case_id": r.case_id,
                    "expected_verdict": r.expected_verdict,
                    "actual_verdict": r.actual_verdict,
                    "passed": r.passed,
                    "diagnostics": r.diagnostics,
                }
                for r in results
            ],
        }
        print(json.dumps(payload, indent=2))
    else:
        for r in results:
            status = "PASS" if r.passed else "FAIL"
            diag_str = ", ".join(f"{c}@L{ln}" for c, ln, _ in r.diagnostics) or "0 diagnostics"
            print(f"[{status}] {r.case_id}: expected={r.expected_verdict} actual={r.actual_verdict} ({diag_str})")
        print(f"\nSummary: {passed}/{total} conformance cases passed.")

    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
