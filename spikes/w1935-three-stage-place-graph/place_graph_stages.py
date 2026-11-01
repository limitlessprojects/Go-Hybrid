"""W-1935 (#1935): Place-Graph Lowering in Three Provable Stages."""

from __future__ import annotations
from dataclasses import dataclass
from typing import List, Set, Tuple


@dataclass(frozen=True)
class PlaceProj:
    kind: str  # "Paren" | "Field" | "SliceIndex" | "MapIndex"
    name: str = ""


@dataclass(frozen=True)
class PlaceExpr:
    root: str
    projections: Tuple[PlaceProj, ...] = ()
    op: str = "read"  # "read" | "addr_of" | "move" | "method_mut_recv" | "closure_capture"


def stage1_erase_wrappers(expr: PlaceExpr) -> PlaceExpr:
    """Stage 1: Pure normalization removing Expr::Paren wrappers."""
    clean = tuple(p for p in expr.projections if p.kind != "Paren")
    return PlaceExpr(expr.root, clean, expr.op)


def verify_place_stages(exprs: List[PlaceExpr], enable_stage3_field_captures: bool = True) -> Tuple[bool, str]:
    """Run Stage 1 (wrapper erasure), Stage 2 (Place/Proj #1775/#1822), and Stage 3 (closure capture places)."""
    moved_roots: Set[str] = set()
    moved_field_paths: Set[Tuple[str, ...]] = set()
    captured_roots: Set[str] = set()
    captured_field_paths: Set[Tuple[str, ...]] = set()

    for raw in exprs:
        e = stage1_erase_wrappers(raw)
        path = (e.root,) + tuple(f"{p.kind}:{p.name}" for p in e.projections)

        # Stage 2 Rule A (#1775): Address-of through a MapIndex projection or its sub-projections is unaddressable in Go
        if e.op in ("addr_of", "method_mut_recv"):
            if any(p.kind == "MapIndex" for p in e.projections):
                return False, "W1935_ERR_1775_UNADDRESSABLE_MAP_PROJECTION"

        # Stage 2 Rule B (#1822): Using or auto-borrowing a moved root or overlapping projection
        if e.root in moved_roots or path in moved_field_paths:
            return False, "W1935_ERR_1822_USE_OR_BORROW_AFTER_MOVE"

        if e.op == "move":
            if e.projections:
                if path in captured_field_paths or e.root in captured_roots:
                    return False, "W1935_ERR_MOVE_CONFLICTS_WITH_CLOSURE_CAPTURE"
                moved_field_paths.add(path)
            else:
                if e.root in captured_roots or any(p[0] == e.root for p in captured_field_paths):
                    return False, "W1935_ERR_MOVE_CONFLICTS_WITH_CLOSURE_CAPTURE"
                moved_roots.add(e.root)

        elif e.op == "closure_capture":
            if enable_stage3_field_captures and e.projections:
                if path in moved_field_paths or e.root in moved_roots:
                    return False, "W1935_ERR_CLOSURE_CAPTURES_MOVED_PLACE"
                captured_field_paths.add(path)
            else:
                if e.root in moved_roots or any(p[0] == e.root for p in moved_field_paths):
                    return False, "W1935_ERR_STAGE2_CONSERVATIVE_WHOLE_ROOT_CAPTURE"
                captured_roots.add(e.root)

    return True, "OK"
