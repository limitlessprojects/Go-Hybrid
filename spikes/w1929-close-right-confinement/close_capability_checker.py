"""W-1929 (#1929): Close-Right Confinement via Linear CloseRight Capability."""

from __future__ import annotations
from dataclasses import dataclass
from typing import Dict, List, Set


@dataclass(frozen=True)
class ChanOp:
    kind: str  # "make_chan" | "spawn_sender" | "join_all" | "close" | "send" | "escape_close_closure" | "escape_close_select"
    chan: str
    task_id: str = "main"
    transfers_close_right: bool = False


@dataclass(frozen=True)
class CloseCheckVerdict:
    ok: bool
    code: str
    reason: str


def verify_close_confinement(ops: List[ChanOp]) -> CloseCheckVerdict:
    """Verify linear CloseRight ownership and post-join / single-owner close safety."""
    close_owner: Dict[str, str] = {}
    active_remote_senders: Dict[str, Set[str]] = {}
    closed_channels: Set[str] = set()

    for op in ops:
        ch = op.chan
        if op.kind == "make_chan":
            close_owner[ch] = op.task_id
            active_remote_senders[ch] = set()
            continue

        if ch not in close_owner:
            return CloseCheckVerdict(False, "W1929_ERR_UNKNOWN_CHANNEL", f"unknown channel '{ch}'")

        if op.kind == "escape_close_closure":
            return CloseCheckVerdict(
                False, "W1929_ERR_CLOSURE_CLOSE_ESCAPE", f"linear CloseRight for '{ch}' cannot escape via closure capture"
            )

        if op.kind == "escape_close_select":
            return CloseCheckVerdict(
                False, "W1929_ERR_SELECT_CLOSE_ESCAPE", f"linear CloseRight for '{ch}' cannot be split across select arms"
            )

        if op.kind == "spawn_sender":
            if op.transfers_close_right:
                if close_owner[ch] != "main" or len(active_remote_senders[ch]) > 0:
                    return CloseCheckVerdict(
                        False,
                        "W1929_ERR_DUPLICATE_CLOSE_HOLDER",
                        f"cannot transfer CloseRight of '{ch}' to '{op.task_id}' while other senders or close holders exist",
                    )
                close_owner[ch] = op.task_id
            active_remote_senders[ch].add(op.task_id)
            continue

        if op.kind == "join_all":
            active_remote_senders[ch].clear()
            continue

        if op.kind == "close":
            if ch in closed_channels:
                return CloseCheckVerdict(False, "W1929_ERR_DOUBLE_CLOSE", f"channel '{ch}' closed twice")
            if close_owner[ch] != op.task_id:
                return CloseCheckVerdict(
                    False,
                    "W1929_ERR_CLOSE_WITHOUT_RIGHT",
                    f"task '{op.task_id}' called close({ch}) without holding linear CloseRight (held by '{close_owner[ch]}')",
                )
            other_senders = active_remote_senders[ch] - {op.task_id}
            if other_senders:
                return CloseCheckVerdict(
                    False,
                    "W1929_ERR_CLOSE_WHILE_SENDERS_LIVE",
                    f"task '{op.task_id}' closed '{ch}' while unjoined senders {sorted(other_senders)} are still live",
                )
            closed_channels.add(ch)
            continue

        if op.kind == "send":
            if ch in closed_channels:
                return CloseCheckVerdict(False, "W1929_ERR_SEND_AFTER_CLOSE", f"send on already-closed channel '{ch}'")
            if close_owner[ch] != op.task_id and close_owner[ch] in active_remote_senders[ch]:
                return CloseCheckVerdict(
                    False,
                    "W1929_ERR_CONCURRENT_CLOSE_HOLDER_RACE",
                    f"sender '{op.task_id}' races with concurrent task '{close_owner[ch]}' holding CloseRight",
                )

    return CloseCheckVerdict(True, "OK", "linear CloseRight confinement verified")
