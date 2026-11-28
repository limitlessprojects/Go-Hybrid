"""Static admission-logic model for Wildcard #4843: `seal move x` -> `sealed[T]`
(Rule O5.7 / Sendable Class 4b).

This is a small, directly-executable model of the three admission checks a
real Tier-0 checker (`compiler/crates/hy-check`) would run before accepting a
`seal move x` expression:

  1. Exclusivity (HYB-E3004): `x` must be exclusively owned at the seal site
     — non-owned parameters/captures are rejected.
  2. Interior sealability (HYB-E3015, declared falsifier F3): `x`'s
     representation must not recursively contain a closure, channel,
     interface, or pointer — those could still mutate state after sealing.
  3. Storage-component exclusivity (HYB-E3003, declared falsifier F2): the
     `freeze.rs:43` `aliases(x) union may_share(x)` set must be empty of
     other *live* bindings — a pre-seal alias/sub-slice that outlives the
     seal is exactly what `work/sealed_test.go`'s TestF2 shows a runtime-only
     implementation cannot defend against; this check is what must catch it
     statically instead.

It also models the `corpus #19` twin from the proposal (plain `move` still
bans sender reads; `seal move` is the zero-copy escape) and the
GUARANTEES.md Rows 5/9 foreign-callback-capture flip (HYB-E3013).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, FrozenSet, Tuple

NON_SEALABLE_KINDS = frozenset({"func", "chan", "interface", "ptr", "unsafe_ptr", "managed_tainted"})


class SealError(Exception):
    """A rejected `seal move` (or foreign-callback capture) with its HYB code."""

    def __init__(self, code: str, message: str) -> None:
        self.code = code
        self.message = message
        super().__init__(f"{code}: {message}")


@dataclass(frozen=True)
class TypeDesc:
    kind: str
    fields: Tuple["TypeDesc", ...] = ()


def is_sealable(t: TypeDesc) -> bool:
    """Recursive representation-purity check (R3)."""
    if t.kind in NON_SEALABLE_KINDS:
        return False
    return all(is_sealable(f) for f in t.fields)


@dataclass
class Binding:
    name: str
    type: TypeDesc
    owned: bool = True
    aliases: FrozenSet[str] = field(default_factory=frozenset)
    may_share: FrozenSet[str] = field(default_factory=frozenset)
    is_const: bool = False


def seal_move(binding: Binding, live_bindings: Dict[str, "Binding"]) -> str:
    """Model `seal move x` admission. Returns the frozen witness name, or
    raises SealError with the HYB code a real diagnostic would carry."""
    if not binding.owned:
        raise SealError("HYB-E3004", f"'{binding.name}' is not exclusively owned (non-owned param/capture)")

    if not is_sealable(binding.type):
        raise SealError(
            "HYB-E3015",
            f"'{binding.name}' has a non-sealable interior (closure/chan/interface/ptr)",
        )

    storage_component = binding.aliases | binding.may_share
    live_others = {n for n in storage_component if n != binding.name and n in live_bindings}
    if live_others:
        raise SealError(
            "HYB-E3003",
            f"'{binding.name}' has live aliases/may_share in its storage component: {sorted(live_others)}",
        )

    return f"sealed({binding.name})"


def move_then_read(binding: Binding) -> None:
    """Model of plain `move x` (corpus #19): `results <- move agg; n := len(agg)`.
    Any post-move read of the moved-from binding is rejected — this must
    stay REJECT independent of `seal move` (D1's §9.2 half is unweakened)."""
    raise SealError("HYB-E3003", f"read of '{binding.name}' after it was sent via plain `move`")


def seal_move_twin(binding: Binding, live_bindings: Dict[str, Binding]) -> str:
    """Model of the zero-copy twin: `let snap = seal move agg; results <- snap;
    n := snap.len()`. Both sender and receivers may keep reading `snap`."""
    return seal_move(binding, live_bindings)


def foreign_callback_capture(captured: Binding, live_bindings: Dict[str, Binding]) -> str:
    """Model of the GUARANTEES.md Rows 5 & 9 flip: a reference capture handed
    to a foreign Go API boundary (e.g. `net/http`'s `http.HandleFunc`) must be
    `const` or sealed — HYB-E3013 otherwise."""
    if captured.is_const:
        return f"ACCEPT (Class 4 const capture: '{captured.name}')"
    try:
        witness = seal_move(captured, live_bindings)
    except SealError as inner:
        raise SealError(
            "HYB-E3013",
            f"foreign-callback capture of '{captured.name}' is neither const nor sealed ({inner.code}: {inner.message})",
        ) from inner
    return f"ACCEPT (sealed capture: {witness})"
