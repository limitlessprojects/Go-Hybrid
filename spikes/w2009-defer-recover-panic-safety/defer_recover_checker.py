"""W-2009 (#2009): Same-Goroutine recover() & Defer Panic-Safety Checker."""

from __future__ import annotations
from dataclasses import dataclass
from typing import List, Set, Tuple


@dataclass(frozen=True)
class DeferStmt:
    kind: str  # "task_move" | "defer_literal" | "defer_close" | "chan_send" | "plain_recover"
    target: str = ""
    captures: Tuple[str, ...] = ()
    calls_recover_directly: bool = False
    calls_helper_with_recover: bool = False
    in_loop: bool = False
    lower_via_exit_channel: bool = False


@dataclass(frozen=True)
class DeferCheckResult:
    ok: bool
    code: str
    reason: str
    recovered_panic_value: str | None = None


def check_defer_panic_safety(stmts: List[DeferStmt], simulated_panic: str | None = None) -> DeferCheckResult:
    """Validate the 6 same-goroutine defer/recover rules from #2009."""
    moved_bindings: Set[str] = set()
    defer_closed_channels: Set[str] = set()
    has_direct_recover_defer = False

    for st in stmts:
        if st.kind == "task_move":
            moved_bindings.add(st.target)
            continue

        if st.kind == "plain_recover":
            return DeferCheckResult(
                False,
                "W2009_ERR_RECOVER_OUTSIDE_DIRECT_DEFER",
                "recover() outside a direct deferred function literal always returns nil by Go spec",
            )

        if st.kind in ("defer_literal", "defer_close"):
            if st.in_loop:
                return DeferCheckResult(
                    False,
                    "W2009_ERR_UNBOUNDED_LOOP_DEFER",
                    "defer inside a loop accumulates unbounded defer frames until function exit",
                )
            for cap in st.captures:
                if cap in moved_bindings:
                    return DeferCheckResult(
                        False,
                        "W2009_ERR_DEFER_CAPTURES_MOVED_BINDING",
                        f"defer closure captures '{cap}' already moved to a task at defer site",
                    )

        if st.kind == "defer_literal":
            if st.calls_helper_with_recover:
                return DeferCheckResult(
                    False,
                    "W2009_ERR_INDIRECT_HELPER_RECOVER",
                    "recover() inside a plain helper called by a deferred function returns nil at runtime",
                )
            if st.calls_recover_directly:
                if st.lower_via_exit_channel:
                    return DeferCheckResult(
                        False,
                        "W2009_ERR_EXIT_CHANNEL_BREAKS_RECOVER",
                        "exit-channel defer lowering runs on another goroutine where recover() is nil",
                        recovered_panic_value=None,
                    )
                has_direct_recover_defer = True

        if st.kind == "defer_close":
            defer_closed_channels.add(st.target)

        if st.kind == "chan_send":
            if st.target in defer_closed_channels:
                return DeferCheckResult(
                    False,
                    "W2009_ERR_SEND_AFTER_DEFER_CLOSE",
                    f"defer close({st.target}) followed by send on '{st.target}' in same block risks send/close ordering bug",
                )

    recovered = simulated_panic if (simulated_panic and has_direct_recover_defer) else None
    return DeferCheckResult(True, "OK", "same-goroutine defer/recover invariants satisfied", recovered)
