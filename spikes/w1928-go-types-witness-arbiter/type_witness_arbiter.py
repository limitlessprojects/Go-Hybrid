"""W-1928 (#1928): A go/types Witness that Arbitrates the Shape Oracle."""

from __future__ import annotations
from dataclasses import dataclass
from typing import Dict, List, Tuple


@dataclass(frozen=True)
class ExprProbe:
    file: str
    span: Tuple[int, int]
    shape_type: str  # "Unknown" or concrete type from shape.rs
    witness_type: str  # "Unknown" or concrete type from go/types
    at_spawn_boundary: bool = False
    bug_ref: str = ""


@dataclass(frozen=True)
class ArbitrationReport:
    resolved_types: Dict[Tuple[str, Tuple[int, int]], str]
    emitter_types: Dict[Tuple[str, Tuple[int, int]], str]
    unknown_recovery_ratio: float
    disagreements: List[str]
    spawn_failures: List[str]
    emitter_checker_parity: bool


def arbitrate_with_witness(probes: List[ExprProbe], arbitration_enabled: bool = True) -> ArbitrationReport:
    """Arbitrate hand-rolled TypeShape against serialized go/types TypeWitness."""
    resolved: Dict[Tuple[str, Tuple[int, int]], str] = {}
    emitter: Dict[Tuple[str, Tuple[int, int]], str] = {}
    disagreements: List[str] = []
    spawn_failures: List[str] = []

    total_unknown_shapes = 0
    recovered_unknowns = 0

    for p in probes:
        key = (p.file, p.span)
        if p.shape_type == "Unknown":
            total_unknown_shapes += 1

        if not arbitration_enabled:
            final_ty = p.shape_type
        else:
            if p.shape_type == "Unknown":
                if p.witness_type != "Unknown":
                    final_ty = p.witness_type
                    recovered_unknowns += 1
                else:
                    final_ty = "Unknown"
            else:
                if p.witness_type != "Unknown" and p.witness_type != p.shape_type:
                    tag = f"{p.file}:{p.span[0]}..{p.span[1]} shape={p.shape_type} vs witness={p.witness_type}"
                    if p.bug_ref:
                        tag += f" ({p.bug_ref})"
                    disagreements.append(tag)
                    final_ty = p.witness_type
                else:
                    final_ty = p.shape_type

        if p.at_spawn_boundary and final_ty in ("Unknown", "[]byte", "map[string]int"):
            spawn_failures.append(f"HYB-E3005:{p.file}:{p.span[0]}")

        resolved[key] = final_ty
        # When arbitration is enabled, hy-emit reads the same resolved witness table
        emitter[key] = final_ty if arbitration_enabled else p.shape_type

    recovery_ratio = round(recovered_unknowns / total_unknown_shapes, 3) if total_unknown_shapes else 0.0
    parity = resolved == emitter
    return ArbitrationReport(
        resolved_types=resolved,
        emitter_types=emitter,
        unknown_recovery_ratio=recovery_ratio,
        disagreements=disagreements,
        spawn_failures=spawn_failures,
        emitter_checker_parity=parity,
    )
