"""
Executable Spike for Wildcard Issue #1755:
Zero-Trampoline SSA Register-ABI Bridge & Linear Place-Capability Tokens.

Demonstrates:
1. Prompt 3 Replacement: O(|V| + |E|) Intra-function Linear Place-Capability Token
   Verification without building an exponential path-sensitive alias graph.
2. Prompt 2 Replacement: Direct Register-ABI Slice Descriptor Validation (ptr, len, cap)
   without C-ABI staticlib conversion or runtime bounds checks inside hot inner loops.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional, Set, Tuple


class Perm(Enum):
    NONE = "none"
    SHARED = "shared"      # Read-only capability, duplicable
    UNIQUE = "unique"      # Read/Write linear capability, non-duplicable
    FROZEN = "frozen"      # Temporarily locked while sub-capabilities exist


@dataclass(frozen=True)
class CapToken:
    token_id: str
    place: str
    perm: Perm
    parent_id: Optional[str] = None
    active_children: Tuple[str, ...] = ()


@dataclass
class SliceRegABI:
    """Go 1.17+ register-ABI 3-tuple slice representation: (ptr_reg, len_reg, cap_reg)."""
    place: str
    ptr_reg: str
    len_reg: str
    cap_reg: str
    guaranteed_non_null: bool = True
    bounds_checked_at_entry: bool = True


class LinearCapabilityVerifier:
    """
    O(|V| + |E|) intra-function verifier replacing path-sensitive alias graphs (Prompt 3)
    and verifying zero-trampoline register-ABI slice invariants (Prompt 2).
    """

    def __init__(self) -> None:
        self.tokens: Dict[str, CapToken] = {}
        self.place_owner: Dict[str, str] = {}
        self.errors: List[str] = []

    def mint_entry_unique(self, token_id: str, place: str) -> CapToken:
        tok = CapToken(token_id=token_id, place=place, perm=Perm.UNIQUE)
        self.tokens[token_id] = tok
        self.place_owner[place] = token_id
        return tok

    def mint_entry_shared(self, token_id: str, place: str) -> CapToken:
        tok = CapToken(token_id=token_id, place=place, perm=Perm.SHARED)
        self.tokens[token_id] = tok
        return tok

    def split_disjoint_slices(
        self,
        parent_token_id: str,
        left_id: str,
        right_id: str,
        split_expr: str,
    ) -> Tuple[Optional[CapToken], Optional[CapToken]]:
        """
        Splits a UNIQUE capability for s[0:n] into two disjoint UNIQUE capabilities:
        s[0:mid] and s[mid:n], freezing the parent capability until re-join.
        """
        parent = self.tokens.get(parent_token_id)
        if parent is None:
            self.errors.append(f"Unknown parent token: {parent_token_id}")
            return None, None
        if parent.perm != Perm.UNIQUE:
            self.errors.append(
                f"Cannot split non-unique token {parent_token_id} (perm={parent.perm.value})"
            )
            return None, None

        frozen_parent = CapToken(
            token_id=parent.token_id,
            place=parent.place,
            perm=Perm.FROZEN,
            parent_id=parent.parent_id,
            active_children=(left_id, right_id),
        )
        self.tokens[parent_token_id] = frozen_parent

        left_place = f"{parent.place}[0:{split_expr}]"
        right_place = f"{parent.place}[{split_expr}:len]"
        left_tok = CapToken(
            token_id=left_id,
            place=left_place,
            perm=Perm.UNIQUE,
            parent_id=parent_token_id,
        )
        right_tok = CapToken(
            token_id=right_id,
            place=right_place,
            perm=Perm.UNIQUE,
            parent_id=parent_token_id,
        )
        self.tokens[left_id] = left_tok
        self.tokens[right_id] = right_tok
        self.place_owner[left_place] = left_id
        self.place_owner[right_place] = right_id
        return left_tok, right_tok

    def rejoin_slices(
        self, parent_token_id: str, left_id: str, right_id: str, new_parent_id: str
    ) -> Optional[CapToken]:
        parent = self.tokens.get(parent_token_id)
        left = self.tokens.get(left_id)
        right = self.tokens.get(right_id)
        if not parent or not left or not right:
            self.errors.append("Missing token during rejoin_slices")
            return None
        if parent.perm != Perm.FROZEN or parent.active_children != (left_id, right_id):
            self.errors.append(f"Invalid frozen parent state on {parent_token_id}")
            return None
        if left.perm != Perm.UNIQUE or right.perm != Perm.UNIQUE:
            self.errors.append("Cannot rejoin consumed or non-unique child tokens")
            return None

        # Consume children
        self.tokens[left_id] = CapToken(left.token_id, left.place, Perm.NONE, parent_token_id)
        self.tokens[right_id] = CapToken(right.token_id, right.place, Perm.NONE, parent_token_id)

        restored = CapToken(
            token_id=new_parent_id,
            place=parent.place,
            perm=Perm.UNIQUE,
            parent_id=parent.parent_id,
        )
        self.tokens[new_parent_id] = restored
        self.place_owner[parent.place] = new_parent_id
        return restored

    def write_with_capability(self, token_id: str, new_token_id: str, place: str) -> Optional[CapToken]:
        tok = self.tokens.get(token_id)
        if tok is None:
            self.errors.append(f"Write attempted with non-existent token {token_id}")
            return None
        if tok.perm != Perm.UNIQUE:
            self.errors.append(
                f"Aliasing/Permission violation: write to {place} using {token_id} with perm={tok.perm.value}"
            )
            return None
        if tok.place != place:
            self.errors.append(
                f"Place mismatch: token {token_id} governs {tok.place}, not {place}"
            )
            return None

        # Linearly consume old token and produce evolved token
        self.tokens[token_id] = CapToken(tok.token_id, tok.place, Perm.NONE, tok.parent_id)
        next_tok = CapToken(new_token_id, place, Perm.UNIQUE, tok.parent_id)
        self.tokens[new_token_id] = next_tok
        self.place_owner[place] = new_token_id
        return next_tok

    def verify_phi_merge(
        self, branch_tokens: List[str], merged_token_id: str, place: str
    ) -> Optional[CapToken]:
        """
        Rule 3 (SSA Phi-Conservation): All incoming CFG edges must carry identical
        permission states for the same canonical place, and UNIQUE tokens from each
        mutually exclusive branch are merged into a single phi token.
        """
        if not branch_tokens:
            self.errors.append("Empty phi node")
            return None

        perms: Set[Perm] = set()
        for tid in branch_tokens:
            tok = self.tokens.get(tid)
            if tok is None:
                self.errors.append(f"Unknown branch token {tid} in phi merge")
                return None
            if tok.place != place:
                self.errors.append(
                    f"Phi place mismatch: {tid} governs {tok.place}, expected {place}"
                )
                return None
            perms.add(tok.perm)

        if len(perms) != 1 or Perm.NONE in perms or Perm.FROZEN in perms:
            self.errors.append(
                f"Phi conservation violation on {place}: inconsistent branch permissions {[p.value for p in perms]}"
            )
            return None

        perm = next(iter(perms))
        for tid in branch_tokens:
            old = self.tokens[tid]
            self.tokens[tid] = CapToken(old.token_id, old.place, Perm.NONE, old.parent_id)

        merged = CapToken(merged_token_id, place, perm)
        self.tokens[merged_token_id] = merged
        if perm == Perm.UNIQUE:
            self.place_owner[place] = merged_token_id
        return merged

    def verify_zero_trampoline_call(
        self,
        slice_abi: SliceRegABI,
        cap_token_id: str,
        callee_requires_mut: bool,
        stack_headroom_bytes: int,
        callee_max_frame_bytes: int,
    ) -> Dict[str, object]:
        """
        Verifies whether a cross-core Go -> Go-Hybrid call can bypass the C-ABI
        staticlib trampoline and execute via direct register-ABI jump/call.
        """
        tok = self.tokens.get(cap_token_id)
        if tok is None or tok.place != slice_abi.place:
            self.errors.append("Invalid capability token for zero-trampoline call")
            return {"direct_reg_abi": False, "reason": "invalid_capability"}

        if callee_requires_mut and tok.perm != Perm.UNIQUE:
            self.errors.append(
                f"Mutable zero-trampoline call requires UNIQUE capability, got {tok.perm.value}"
            )
            return {"direct_reg_abi": False, "reason": "permission_violation"}

        can_skip_morestack = stack_headroom_bytes >= callee_max_frame_bytes
        elide_inner_bounds_checks = (
            slice_abi.guaranteed_non_null and slice_abi.bounds_checked_at_entry
        )

        return {
            "direct_reg_abi": True,
            "c_abi_trampoline_instructions": 0,
            "registers_mapped": (slice_abi.ptr_reg, slice_abi.len_reg, slice_abi.cap_reg),
            "elide_morestack_prologue": can_skip_morestack,
            "elide_inner_loop_bounds_checks": elide_inner_bounds_checks,
        }
