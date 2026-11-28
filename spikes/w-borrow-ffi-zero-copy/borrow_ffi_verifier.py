#!/usr/bin/env python3
"""Tier-0 verifier model for synchronous zero-copy FFI crossings (borrow_ffi[T])."""

from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass(frozen=True)
class BorrowFfiCallSite:
    name: str
    elem_has_go_pointers: bool = False
    view_escapes_via: Optional[str] = None
    backing_mutated_in_window: bool = False
    is_subslice: bool = False
    three_index_clipped: bool = True


def verify_borrow_ffi_site(site: BorrowFfiCallSite) -> Tuple[bool, List[str]]:
    """Return (accepted, diagnostics) under Tier-0 A2 + HYB-E3501..HYB-E3504 rules."""
    diags: List[str] = []
    if site.elem_has_go_pointers:
        diags.append(
            f"HYB-E3504 [{site.name}]: borrow_ffi element type contains Go pointers; "
            "only pointer-free slices/strings may cross via zero-copy view"
        )
    if site.view_escapes_via is not None:
        diags.append(
            f"HYB-E3501 [{site.name}]: borrowed FFI view escapes synchronous call "
            f"frame via {site.view_escapes_via}"
        )
    if site.backing_mutated_in_window:
        diags.append(
            f"HYB-E3502 [{site.name}]: backing Go slice mutated or reallocated "
            "inside active borrow_ffi window (A2 freeze violation)"
        )
    if site.is_subslice and not site.three_index_clipped:
        diags.append(
            f"HYB-E3503 [{site.name}]: sub-slice passed to borrow_ffi without "
            "3-index capacity clip s[lo:hi:hi] (spare capacity bleed)"
        )
    return (len(diags) == 0, diags)


PRE_REGISTERED_CASES: Tuple[Tuple[BorrowFfiCallSite, bool, Optional[str]], ...] = (
    (
        BorrowFfiCallSite(name="P1_sync_sha3_digest_clipped", is_subslice=True, three_index_clipped=True),
        True,
        None,
    ),
    (
        BorrowFfiCallSite(name="P4a_callback_append_realloc", backing_mutated_in_window=True),
        False,
        "HYB-E3502",
    ),
    (
        BorrowFfiCallSite(name="P4b_unclipped_subslice_bleed", is_subslice=True, three_index_clipped=False),
        False,
        "HYB-E3503",
    ),
    (
        BorrowFfiCallSite(name="P4c_async_goroutine_capture", view_escapes_via="spawn_task"),
        False,
        "HYB-E3501",
    ),
    (
        BorrowFfiCallSite(name="F4_nested_go_pointer_slice", elem_has_go_pointers=True),
        False,
        "HYB-E3504",
    ),
)
