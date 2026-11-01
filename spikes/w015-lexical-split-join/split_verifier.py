#!/usr/bin/env python3
"""W-015 (#294) Spike: Lexical Split-Join Slice Capabilities via 3-Index Capacity Clipping."""

from dataclasses import dataclass, field
from typing import Dict, List, Set, Tuple

REF_ELEM_PREFIXES = ("map[", "[]", "*", "chan ", "func(")


@dataclass
class SliceView:
    backing: List[int]
    offset: int
    length: int
    capacity: int

    def read(self, idx: int) -> int:
        return self.backing[self.offset + idx]

    def write(self, idx: int, val: int) -> None:
        self.backing[self.offset + idx] = val

    def append_val(self, val: int) -> "SliceView":
        """Simulates Go runtime append: in-place if length < capacity, else fresh backing array."""
        if self.length < self.capacity:
            self.backing[self.offset + self.length] = val
            return SliceView(self.backing, self.offset, self.length + 1, self.capacity)
        fresh = [self.backing[self.offset + i] for i in range(self.length)] + [val]
        return SliceView(fresh, 0, len(fresh), len(fresh))


def split_three_index(s: SliceView, mid: int) -> Tuple[SliceView, SliceView]:
    """Lowers `split s at mid into left, right` -> `s[0:mid:mid]` and `s[mid:len(s):cap(s)]`."""
    left = SliceView(s.backing, s.offset, mid, mid)
    right = SliceView(s.backing, s.offset + mid, s.length - mid, s.capacity - mid)
    return left, right


def split_two_index_unsound(s: SliceView, mid: int) -> Tuple[SliceView, SliceView]:
    """Negative control: 2-index `s[:mid]` keeps full `s.capacity`, allowing append to clobber `right[0]`."""
    left = SliceView(s.backing, s.offset, mid, s.capacity)
    right = SliceView(s.backing, s.offset + mid, s.length - mid, s.capacity - mid)
    return left, right


@dataclass
class SplitBinding:
    name: str
    elem_type: str = "int"
    owned: bool = True
    is_const: bool = False
    has_live_sharers: bool = False
    moved: bool = False


@dataclass
class SplitCheckResult:
    verdict: str
    diagnostics: List[str] = field(default_factory=list)
    emitted_go: List[str] = field(default_factory=list)


def check_split_program(ops: List[Dict[str, object]]) -> SplitCheckResult:
    bindings: Dict[str, SplitBinding] = {}
    diags: List[str] = []
    emitted: List[str] = []

    def walk(stmts: List[Dict[str, object]], scope_bindings: Set[str], split_locked: Set[str]) -> None:
        local_scope = set(scope_bindings)
        for st in stmts:
            kind = str(st.get("op", ""))
            if kind == "decl_slice":
                name = str(st["name"])
                bindings[name] = SplitBinding(
                    name=name,
                    elem_type=str(st.get("elem_type", "int")),
                    owned=bool(st.get("owned", True)),
                    is_const=bool(st.get("is_const", False)),
                    has_live_sharers=bool(st.get("has_live_sharers", False)),
                )
                local_scope.add(name)

            elif kind == "use":
                target = str(st["target"])
                if target in split_locked:
                    diags.append(f"HYB-E3106: base slice '{target}' is split-out and inaccessible inside split block")
                elif target not in local_scope:
                    diags.append(f"HYB-E1001: undefined or out-of-scope split fork '{target}'")
                elif bindings.get(target) and bindings[target].moved:
                    diags.append(f"HYB-E3003: use of moved slice binding '{target}'")

            elif kind == "move_task":
                target = str(st["target"])
                if target in split_locked:
                    diags.append(f"HYB-E3106: cannot move split-out base slice '{target}'")
                elif target not in local_scope:
                    diags.append(f"HYB-E1001: undefined binding '{target}'")
                else:
                    b = bindings[target]
                    if b.moved:
                        diags.append(f"HYB-E3003: slice fork '{target}' moved more than once")
                    b.moved = True

            elif kind == "escape_outer_chan":
                target = str(st["target"])
                diags.append(f"HYB-E3004: partitioned slice fork '{target}' cannot escape lexical split block")

            elif kind == "split":
                base = str(st["base"])
                mid = str(st.get("mid", "mid"))
                left = str(st["left"])
                right = str(st["right"])
                if base in split_locked:
                    diags.append(f"HYB-E3106: slice '{base}' is already split-out in enclosing block")
                    continue
                b = bindings.get(base)
                if b is None or base not in local_scope:
                    diags.append(f"HYB-E1001: unknown slice '{base}'")
                    continue
                if not b.owned:
                    diags.append(f"HYB-E3105 (F1a): cannot split non-owned parameter/borrowed slice '{base}'")
                    continue
                if b.is_const:
                    diags.append(f"HYB-E3105 (F1b): cannot split immutable const slice '{base}'")
                    continue
                if b.has_live_sharers or b.moved:
                    diags.append(f"HYB-E3105: cannot split aliased or moved slice '{base}'")
                    continue
                if any(b.elem_type.startswith(p) for p in REF_ELEM_PREFIXES):
                    diags.append(f"HYB-E3105 (F2): element type '{b.elem_type}' carries shared heap references")
                    continue

                emitted.append(f"{left} := {base}[0:{mid}:{mid}]")
                emitted.append(f"{right} := {base}[{mid}:len({base}):cap({base})]")
                bindings[left] = SplitBinding(name=left, elem_type=b.elem_type, owned=True)
                bindings[right] = SplitBinding(name=right, elem_type=b.elem_type, owned=True)
                inner_scope = (local_scope - {base}) | {left, right}
                walk(list(st.get("body", [])), inner_scope, split_locked | {base})
                bindings.pop(left, None)
                bindings.pop(right, None)

    walk(ops, set(), set())
    return SplitCheckResult(verdict=("ACCEPT" if not diags else "REJECT"), diagnostics=diags, emitted_go=emitted)
