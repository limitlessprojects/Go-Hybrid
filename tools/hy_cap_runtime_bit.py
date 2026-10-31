#!/usr/bin/env python3
"""W-003 (#193) Spike: Linear Capability Types (own/share/view) + 1-Bit Runtime Header Guard.

Replaces frame-by-frame alias/loan/freeze reconstruction with:
1. Local per-function capability typing (`own[T]`, `share[T]`, `view[T]` or Go `hyb.Own[T]`).
2. One runtime ownership bit per composite header (`0x1` moved, `0x2` Rust FFI transferred)
   that catches ambient Go aliasing and `const` copies (#37/#44/#110/#111/#123) at the boundary.
"""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set

OWNERSHIP_BIT_LIVE = 0x0
OWNERSHIP_BIT_MOVED = 0x1
OWNERSHIP_BIT_FFI_RUST = 0x2


@dataclass
class CompositeHeader:
    """Simulates a 64-bit composite header reusing 1 padding byte for ownership flags (0 B extra)."""
    header_id: str
    flags_bit: int = OWNERSHIP_BIT_LIVE
    active_views: int = 0
    padding_bytes_added: int = 0

    def flip_boundary_bit(self) -> Optional[str]:
        if self.flags_bit & OWNERSHIP_BIT_FFI_RUST:
            return f"HYB-R3002: panic: hyb: composite '{self.header_id}' owned by Rust FFI bridge"
        if self.flags_bit & OWNERSHIP_BIT_MOVED:
            return f"HYB-R3001: panic: hyb: use of moved value '{self.header_id}' at boundary"
        if self.active_views > 0:
            return f"HYB-E3004: cannot move '{self.header_id}' while view[T] loan is active"
        self.flags_bit |= OWNERSHIP_BIT_MOVED
        return None

    def transfer_to_rust(self) -> Optional[str]:
        err = self.flip_boundary_bit()
        if err:
            return err
        self.flags_bit |= OWNERSHIP_BIT_FFI_RUST
        return None

    def reclaim_from_rust(self) -> None:
        self.flags_bit = OWNERSHIP_BIT_LIVE

    def check_access(self, is_mutation: bool = False) -> Optional[str]:
        if self.flags_bit & OWNERSHIP_BIT_FFI_RUST:
            return f"HYB-R3002: panic: hyb: access to '{self.header_id}' while transferred to Rust FFI"
        if self.flags_bit & OWNERSHIP_BIT_MOVED:
            return f"HYB-R3001: panic: hyb: use of moved value '{self.header_id}'"
        if is_mutation and self.active_views > 0:
            return f"HYB-E3004: mutation of '{self.header_id}' while view[T] is active"
        return None


@dataclass
class CapBinding:
    name: str
    cap_kind: str  # "own", "share", "view", "ambient"
    header: CompositeHeader
    consumed: bool = False
    share_Frozen: bool = False


@dataclass
class VerificationReport:
    verdict: str
    static_errors: List[str] = field(default_factory=list)
    runtime_traps: List[str] = field(default_factory=list)
    headers: Dict[str, CompositeHeader] = field(default_factory=dict)


def normalize_cap_surface(raw_cap: str) -> str:
    """Supports both .hy (`own[T]`) and W-011 erased .go (`hyb.Own[T]`) capability surfaces."""
    cleaned = raw_cap.strip().lower()
    if cleaned.startswith("hyb."):
        cleaned = cleaned[4:]
    for prefix in ("own", "share", "view", "ambient"):
        if cleaned.startswith(prefix):
            return prefix
    return "own"


def verify_capability_ops(ops: List[Dict[str, object]], in_loop: bool = False) -> VerificationReport:
    headers: Dict[str, CompositeHeader] = {}
    bindings: Dict[str, CapBinding] = {}
    share_peers: Dict[str, Set[str]] = {}
    static_errors: List[str] = []
    runtime_traps: List[str] = []

    passes = 2 if in_loop else 1
    for pass_idx in range(passes):
        for op in ops:
            kind = str(op.get("op", ""))
            if kind == "decl":
                if pass_idx > 0:
                    continue
                name = str(op["name"])
                cap = normalize_cap_surface(str(op.get("cap", "own[map]")))
                hdr_id = str(op.get("header_id", name))
                hdr = headers.setdefault(hdr_id, CompositeHeader(header_id=hdr_id))
                bindings[name] = CapBinding(name=name, cap_kind=cap, header=hdr)
                share_peers.setdefault(hdr_id, set()).add(name)

            elif kind == "rebind":
                dst = str(op["dst"])
                src = str(op["src"])
                mode = str(op.get("mode", "copy"))
                b_src = bindings.get(src)
                if b_src is None:
                    static_errors.append(f"HYB-E3000: unknown binding '{src}'")
                    continue
                if mode == "copy":
                    if b_src.cap_kind == "own":
                        static_errors.append(
                            f"HYB-E3002: linear own[T] '{src}' cannot be copied to '{dst}' without move or share[T]"
                        )
                    elif b_src.cap_kind == "share":
                        bindings[dst] = CapBinding(name=dst, cap_kind="share", header=b_src.header)
                        share_peers.setdefault(b_src.header.header_id, set()).add(dst)
                    elif b_src.cap_kind == "view":
                        b_src.header.active_views += 1
                        bindings[dst] = CapBinding(name=dst, cap_kind="view", header=b_src.header)
                elif mode == "view":
                    if b_src.consumed or (b_src.header.flags_bit & OWNERSHIP_BIT_MOVED):
                        static_errors.append(f"HYB-E3001: cannot borrow view[T] from moved '{src}'")
                    else:
                        b_src.header.active_views += 1
                        bindings[dst] = CapBinding(name=dst, cap_kind="view", header=b_src.header)
                elif mode == "ambient_alias":
                    # Simulates ambient Go code or const copy aliasing the underlying composite header
                    bindings[dst] = CapBinding(name=dst, cap_kind="ambient", header=b_src.header)

            elif kind in ("move", "send", "task_capture"):
                target = str(op["target"])
                b = bindings.get(target)
                if b is None:
                    static_errors.append(f"HYB-E3000: unknown binding '{target}'")
                    continue
                if b.cap_kind == "view":
                    static_errors.append(f"HYB-E3004: view[T] '{target}' cannot cross task/move boundary")
                    continue
                if b.consumed:
                    code = "HYB-E3003" if in_loop else "HYB-E3001"
                    static_errors.append(f"{code}: linear capability '{target}' already consumed")
                    continue
                if b.cap_kind == "share":
                    for peer_name in share_peers.get(b.header.header_id, set()):
                        if peer_name in bindings:
                            bindings[peer_name].share_Frozen = True
                b.consumed = True
                trap = b.header.flip_boundary_bit()
                if trap:
                    if trap.startswith("HYB-E"):
                        static_errors.append(trap)
                    else:
                        runtime_traps.append(trap)

            elif kind in ("read", "write", "delete"):
                target = str(op["target"])
                b = bindings.get(target)
                if b is None:
                    continue
                is_mut = kind in ("write", "delete")
                if b.consumed and b.cap_kind != "ambient":
                    static_errors.append(f"HYB-E3001: use of consumed capability '{target}'")
                if is_mut and b.cap_kind == "view":
                    static_errors.append(f"HYB-E3004: cannot mutate through read-only view[T] '{target}'")
                if is_mut and b.share_Frozen:
                    static_errors.append(f"HYB-E3006: cannot mutate frozen share[T] peer '{target}'")
                trap = b.header.check_access(is_mutation=is_mut)
                if trap:
                    if trap.startswith("HYB-E"):
                        static_errors.append(trap)
                    else:
                        runtime_traps.append(trap)

            elif kind == "ffi_into_rust":
                target = str(op["target"])
                b = bindings[target]
                b.consumed = True
                trap = b.header.transfer_to_rust()
                if trap:
                    runtime_traps.append(trap)

            elif kind == "ffi_from_rust":
                target = str(op["target"])
                b = bindings[target]
                b.consumed = False
                b.header.reclaim_from_rust()

    verdict = "ACCEPT" if not static_errors and not runtime_traps else "REJECT"
    return VerificationReport(
        verdict=verdict,
        static_errors=static_errors,
        runtime_traps=runtime_traps,
        headers=headers,
    )
