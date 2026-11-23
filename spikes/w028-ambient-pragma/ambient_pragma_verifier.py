"""
W-028: Zero-Split Ambient Verification
In-Source Go Pragma-Directed Affine Escape & Sendability Enforcement via go/types Check Hooks

Answering Moonshot Solicitation Prompt 1:
"Kill the .hy / .go split. Whole-module checking, or checking that attaches to ordinary Go via annotations — what would it take, and what guarantee dies with it?"
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum, auto
from typing import Dict, List, Optional, Set, Tuple


class DiagnosticCode(str, Enum):
    USE_AFTER_MOVE = "HYB-E3001"
    ILLEGAL_BORROW_ESCAPE = "HYB-E3002"
    UNCONSUMED_AFFINE_OBLIGATION = "HYB-E3003"
    NON_SENDABLE_CROSS_BOUNDARY = "HYB-E3014"
    UNKNOWN_PRAGMA_DIRECTIVE = "HYB-E1099"


class CapabilityKind(Enum):
    NONE = auto()
    AFFINE = auto()       # At most once consumption (cannot be used after move)
    SENDABLE = auto()     # Safe to transfer across goroutine/channel boundaries
    BORROWED = auto()     # Read-only or exclusive borrow with bounded scope


@dataclass
class TypeDescriptor:
    name: str
    is_affine: bool = False
    is_sendable: bool = False
    is_pointer_or_ref: bool = False
    fields: Dict[str, "TypeDescriptor"] = field(default_factory=dict)

    def is_transitively_sendable(self, visited: Optional[Set[str]] = None) -> bool:
        if self.is_sendable:
            return True
        if self.is_pointer_or_ref and not self.is_affine:
            return False
        if visited is None:
            visited = set()
        if self.name in visited:
            return True
        visited.add(self.name)
        for f in self.fields.values():
            if not f.is_transitively_sendable(visited):
                return False
        return True


@dataclass
class SymbolBinding:
    name: str
    type_desc: TypeDescriptor
    scope_level: int
    is_moved: bool = False
    moved_at_step: int = -1
    borrow_count: int = 0
    is_consumed: bool = False


@dataclass
class Diagnostic:
    code: DiagnosticCode
    symbol: str
    message: str
    line: int


class AmbientPragmaVerifier:
    """
    Verifies Go source programs carrying ambient //go:hyb annotations.
    Analyzes affine linear move semantics, sendability boundaries across
    goroutines (go func), channel sends (ch <- x), and closure captures.
    """

    def __init__(self):
        self.type_registry: Dict[str, TypeDescriptor] = {}
        self.diagnostics: List[Diagnostic] = []
        self._init_builtin_types()

    def _init_builtin_types(self):
        # Primitive scalar types are sendable by value
        for scalar in ["int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16", "uint32", "uint64", "float32", "float64", "bool", "string"]:
            self.type_registry[scalar] = TypeDescriptor(name=scalar, is_sendable=True)

    def register_type(self, desc: TypeDescriptor):
        self.type_registry[desc.name] = desc

    def verify_program(self, ast_operations: List[dict]) -> List[Diagnostic]:
        """
        Executes abstract interpretation of AST operations and pragma directives.
        ast_operations is a sequence of parsed Go statements / declarations with pragma tags.
        """
        self.diagnostics.clear()
        symbols: Dict[str, SymbolBinding] = {}
        step = 0

        for op in ast_operations:
            step += 1
            op_type = op.get("op")
            line = op.get("line", step)

            if op_type == "type_decl":
                # //go:hyb affine / sendable type T struct { ... }
                name = op["name"]
                pragmas = op.get("pragmas", [])
                is_affine = "affine" in pragmas
                is_sendable = "sendable" in pragmas
                fields = {}
                for f_name, f_type_name in op.get("fields", {}).items():
                    f_desc = self.type_registry.get(f_type_name, TypeDescriptor(name=f_type_name, is_pointer_or_ref=("*" in f_type_name or "map" in f_type_name or "chan" in f_type_name)))
                    fields[f_name] = f_desc

                desc = TypeDescriptor(
                    name=name,
                    is_affine=is_affine,
                    is_sendable=is_sendable,
                    is_pointer_or_ref=op.get("is_ptr", False),
                    fields=fields
                )
                self.register_type(desc)

            elif op_type == "var_decl":
                sym_name = op["name"]
                type_name = op["type"]
                type_desc = self.type_registry.get(type_name)
                if not type_desc:
                    type_desc = TypeDescriptor(name=type_name, is_pointer_or_ref=("*" in type_name))
                pragmas = op.get("pragmas", [])
                if "affine" in pragmas:
                    # In-place affine pragma on variable declaration
                    type_desc = TypeDescriptor(
                        name=type_desc.name,
                        is_affine=True,
                        is_sendable=type_desc.is_sendable,
                        is_pointer_or_ref=type_desc.is_pointer_or_ref,
                        fields=type_desc.fields
                    )
                symbols[sym_name] = SymbolBinding(
                    name=sym_name,
                    type_desc=type_desc,
                    scope_level=op.get("scope", 0)
                )

            elif op_type == "use":
                sym_name = op["name"]
                if sym_name in symbols:
                    sym = symbols[sym_name]
                    if sym.is_moved:
                        self.diagnostics.append(Diagnostic(
                            code=DiagnosticCode.USE_AFTER_MOVE,
                            symbol=sym_name,
                            message=f"Use of affine symbol '{sym_name}' after move at step {sym.moved_at_step}",
                            line=line
                        ))

            elif op_type == "move":
                # move(x) or //go:hyb move pragma
                sym_name = op["name"]
                if sym_name in symbols:
                    sym = symbols[sym_name]
                    if sym.is_moved:
                        self.diagnostics.append(Diagnostic(
                            code=DiagnosticCode.USE_AFTER_MOVE,
                            symbol=sym_name,
                            message=f"Cannot move affine symbol '{sym_name}' more than once (already moved at step {sym.moved_at_step})",
                            line=line
                        ))
                    elif sym.type_desc.is_affine:
                        sym.is_moved = True
                        sym.is_consumed = True
                        sym.moved_at_step = step

            elif op_type == "send_chan":
                # ch <- val
                sym_name = op["val"]
                if sym_name in symbols:
                    sym = symbols[sym_name]
                    if sym.is_moved:
                        self.diagnostics.append(Diagnostic(
                            code=DiagnosticCode.USE_AFTER_MOVE,
                            symbol=sym_name,
                            message=f"Cannot send moved symbol '{sym_name}' to channel",
                            line=line
                        ))
                    elif sym.type_desc.is_affine:
                        # Sending affine value moves ownership into channel
                        sym.is_moved = True
                        sym.is_consumed = True
                        sym.moved_at_step = step
                    elif not sym.type_desc.is_transitively_sendable():
                        self.diagnostics.append(Diagnostic(
                            code=DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY,
                            symbol=sym_name,
                            message=f"Type '{sym.type_desc.name}' is not sendable: contains non-sendable pointer or aliased references",
                            line=line
                        ))

            elif op_type == "spawn_goroutine":
                # go func(args) { ... }
                captures = op.get("captures", [])
                args = op.get("args", [])

                for cap_name in captures:
                    if cap_name in symbols:
                        sym = symbols[cap_name]
                        if sym.is_moved:
                            self.diagnostics.append(Diagnostic(
                                code=DiagnosticCode.USE_AFTER_MOVE,
                                symbol=cap_name,
                                message=f"Cannot capture moved symbol '{cap_name}' in goroutine",
                                line=line
                            ))
                        elif sym.type_desc.is_affine:
                            # Affine values captured into goroutine must be moved or rejected
                            if op.get("move_captures", False):
                                sym.is_moved = True
                                sym.is_consumed = True
                                sym.moved_at_step = step
                            else:
                                self.diagnostics.append(Diagnostic(
                                    code=DiagnosticCode.ILLEGAL_BORROW_ESCAPE,
                                    symbol=cap_name,
                                    message=f"Affine symbol '{cap_name}' captured by reference into concurrent goroutine without explicit move",
                                    line=line
                                ))
                        elif not sym.type_desc.is_transitively_sendable():
                            self.diagnostics.append(Diagnostic(
                                code=DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY,
                                symbol=cap_name,
                                message=f"Captured symbol '{cap_name}' of type '{sym.type_desc.name}' escapes to goroutine but is not sendable",
                                line=line
                            ))

                for arg_name in args:
                    if arg_name in symbols:
                        sym = symbols[arg_name]
                        if sym.is_moved:
                            self.diagnostics.append(Diagnostic(
                                code=DiagnosticCode.USE_AFTER_MOVE,
                                symbol=arg_name,
                                message=f"Cannot pass moved symbol '{arg_name}' as goroutine argument",
                                line=line
                            ))
                        elif sym.type_desc.is_affine:
                            sym.is_moved = True
                            sym.is_consumed = True
                            sym.moved_at_step = step
                        elif not sym.type_desc.is_transitively_sendable():
                            self.diagnostics.append(Diagnostic(
                                code=DiagnosticCode.NON_SENDABLE_CROSS_BOUNDARY,
                                symbol=arg_name,
                                message=f"Argument '{arg_name}' of type '{sym.type_desc.name}' passed to goroutine is not sendable",
                                line=line
                            ))

            elif op_type == "scope_exit":
                # Check for unconsumed affine obligations
                scope_lvl = op.get("scope", 0)
                for sym_name, sym in symbols.items():
                    if sym.scope_level == scope_lvl and sym.type_desc.is_affine:
                        if not sym.is_consumed and not sym.is_moved:
                            self.diagnostics.append(Diagnostic(
                                code=DiagnosticCode.UNCONSUMED_AFFINE_OBLIGATION,
                                symbol=sym_name,
                                message=f"Affine obligation '{sym_name}' of type '{sym.type_desc.name}' was not consumed prior to scope exit",
                                line=line
                            ))

        return self.diagnostics
