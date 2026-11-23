"""Compiler-Directed Escape Inversion: Lowering hy-check Lifetime Proofs into Go SSA No-Escape Directives.

Spike model for Go-Hybrid Wildcard Issue #4688.
Demonstrates lowering of hy-check lifetime and non-escape witnesses into Go compiler
pragmas (//go:noescape, //go:uintptr-keepalive) and stack-frame pinning descriptors.
"""

from dataclasses import dataclass, field
from typing import List, Dict, Optional, Tuple, Set
from enum import Enum


class StorageClass(Enum):
    STACK_PINNED = "stack_pinned"
    HEAP_ESCAPING = "heap_escaping"
    REGISTER_SCALAR = "register_scalar"


class BoundaryKind(Enum):
    TASK_SPAWN = "task_spawn"
    FFI_CALL = "ffi_call"
    LEXICAL_SPLIT = "lexical_split"
    CLOSURE_CALL = "closure_call"


@dataclass
class PlaceWitness:
    name: str
    type_name: str
    decl_depth: int
    is_borrow: bool = False
    is_mutable: bool = False
    escapes_lexical_scope: bool = False
    pinned_to_region: Optional[str] = None


@dataclass
class BoundaryInvocation:
    kind: BoundaryKind
    target_fn: str
    call_depth: int
    arguments: List[PlaceWitness]
    region_scope: Optional[str] = None


class EscapeInversionPipeline:
    """Simulates the hy-check -> hy-emit escape inversion analysis and lowering."""

    def __init__(self):
        self.diagnostics: List[Tuple[str, str]] = []

    def classify_place(self, place: PlaceWitness, invocation: BoundaryInvocation) -> StorageClass:
        """Determines whether a place can be proven to remain on the stack at a boundary call."""
        # Check rule HYB-E4688-05: Returning or outliving declaration scope
        if place.escapes_lexical_scope and not place.pinned_to_region:
            self.diagnostics.append(("HYB-E4688-01", f"Place '{place.name}' escapes lexical declaration depth {place.decl_depth}"))
            return StorageClass.HEAP_ESCAPING

        # Check rule HYB-E4688-02: Task spawn without enclosing region anchor
        if invocation.kind == BoundaryKind.TASK_SPAWN:
            if not invocation.region_scope and not place.pinned_to_region:
                self.diagnostics.append(("HYB-E4688-02", f"Async task spawn '{invocation.target_fn}' captures '{place.name}' without bounded region anchor"))
                return StorageClass.HEAP_ESCAPING
            if place.is_mutable and place.pinned_to_region != invocation.region_scope:
                self.diagnostics.append(("HYB-E4688-02", f"Mutable capture '{place.name}' crosses region boundaries"))
                return StorageClass.HEAP_ESCAPING

        # Check rule HYB-E4688-04: FFI boundary constraints
        if invocation.kind == BoundaryKind.FFI_CALL:
            if place.is_borrow and place.decl_depth < invocation.call_depth:
                # Borrowed small/pinned place passed to FFI leaf call
                return StorageClass.STACK_PINNED

        # If certified non-escaping within lexical frame:
        if not place.escapes_lexical_scope:
            return StorageClass.STACK_PINNED

        return StorageClass.HEAP_ESCAPING

    def emit_go_lowering(self, invocation: BoundaryInvocation) -> Dict[str, str]:
        """Emits Go source and assembly stub pragmas according to classification."""
        emitted_go = []
        emitted_asm = []

        fn_name = invocation.target_fn
        param_decls = []
        noescape_params = []

        for p in invocation.arguments:
            sclass = self.classify_place(p, invocation)
            param_decls.append(f"{p.name} *{p.type_name}")
            if sclass == StorageClass.STACK_PINNED:
                noescape_params.append(p.name)

        params_sig = ", ".join(param_decls)

        if noescape_params:
            # Emit //go:noescape pragma declaration with external asm stub anchor
            emitted_go.append(f"//go:noescape")
            emitted_go.append(f"func {fn_name}({params_sig})")
            emitted_asm.append(f"// Statically verified leaf frame for {fn_name}")
            emitted_asm.append(f"// StackPinned parameters: {', '.join(noescape_params)}")
            emitted_asm.append(f"TEXT ·{fn_name}(SB), NOSPLIT, $0-0")
            emitted_asm.append(f"\tRET")
        else:
            # Fallback to standard Go function
            emitted_go.append(f"func {fn_name}({params_sig}) {{")
            emitted_go.append(f"\t// Uncertified or escaping parameters handled by standard GC")
            emitted_go.append(f"}}")

        return {
            "go_source": "\n".join(emitted_go),
            "asm_stub": "\n".join(emitted_asm),
            "stack_pinned_count": len(noescape_params),
            "diagnostics": list(self.diagnostics),
        }

    def simulate_allocations(self, invocation: BoundaryInvocation, iterations: int = 1000) -> Dict[str, int]:
        """Calculates expected allocations under Baseline Go vs Escape-Inverted Go."""
        baseline_allocs = 0
        baseline_bytes = 0
        inverted_allocs = 0
        inverted_bytes = 0

        # Typical sizeof pointer/slice header in Go 64-bit: 24 bytes (slice), 8 bytes (ptr)
        bytes_per_arg = 24

        for _ in range(iterations):
            for p in invocation.arguments:
                sclass = self.classify_place(p, invocation)
                # In standard Go, passing pointers across task/FFI boundaries forces heap escape
                baseline_allocs += 1
                baseline_bytes += bytes_per_arg

                if sclass == StorageClass.HEAP_ESCAPING:
                    inverted_allocs += 1
                    inverted_bytes += bytes_per_arg
                else:
                    # StackPinned: 0 heap allocs, 0 heap bytes
                    pass

        return {
            "baseline_allocs": baseline_allocs,
            "baseline_bytes": baseline_bytes,
            "inverted_allocs": inverted_allocs,
            "inverted_bytes": inverted_bytes,
            "saved_allocs": baseline_allocs - inverted_allocs,
            "saved_bytes": baseline_bytes - inverted_bytes,
        }
