"""W-1931 (#1931): Reserve, Don't Commit — Demand-Paged Deep-Stack Pool Allocator."""

from __future__ import annotations
import math
from dataclasses import dataclass

PAGE_BYTES = 4096
SLOT_VIRTUAL_BYTES = 1024 * 1024  # 1 MiB reserved virtual address space
GUARD_PAGE_BYTES = 4096


@dataclass(frozen=True)
class StackPoolReport:
    concurrent_slots: int
    eager_rss_bytes: int
    demand_paged_rss_bytes: int
    rss_ratio: float
    guard_page_faulted: bool
    status: str  # "OK" | "W1931_ERR_GUARD_PAGE_FAULT"
    stackless_future_state_bytes: int


def evaluate_deep_stack_pool(
    concurrent_slots: int, recursion_depth: int, bytes_per_frame: int = 384
) -> StackPoolReport:
    """Compare eager 1 MiB commit vs reserve-only PROT_NONE + 4 KiB demand paging."""
    usable_virtual = SLOT_VIRTUAL_BYTES - GUARD_PAGE_BYTES
    touched_bytes_per_slot = max(PAGE_BYTES, recursion_depth * bytes_per_frame)
    eager_rss = concurrent_slots * SLOT_VIRTUAL_BYTES

    # Stackless Future state-machine watermark for depth-N recursion (falsifies < 512 B claim at depth 127)
    future_state_bytes = 64 + recursion_depth * 48

    if touched_bytes_per_slot > usable_virtual:
        committed_per_slot = usable_virtual
        demand_rss = concurrent_slots * committed_per_slot
        return StackPoolReport(
            concurrent_slots=concurrent_slots,
            eager_rss_bytes=eager_rss,
            demand_paged_rss_bytes=demand_rss,
            rss_ratio=round(demand_rss / eager_rss, 4),
            guard_page_faulted=True,
            status="W1931_ERR_GUARD_PAGE_FAULT",
            stackless_future_state_bytes=future_state_bytes,
        )

    pages_per_slot = math.ceil(touched_bytes_per_slot / PAGE_BYTES)
    committed_per_slot = pages_per_slot * PAGE_BYTES
    demand_rss = concurrent_slots * committed_per_slot
    ratio = round(demand_rss / eager_rss, 4) if eager_rss else 0.0
    return StackPoolReport(
        concurrent_slots=concurrent_slots,
        eager_rss_bytes=eager_rss,
        demand_paged_rss_bytes=demand_rss,
        rss_ratio=ratio,
        guard_page_faulted=False,
        status="OK",
        stackless_future_state_bytes=future_state_bytes,
    )
