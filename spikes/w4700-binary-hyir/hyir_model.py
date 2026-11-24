#!/usr/bin/env python3
"""
Formal Model and Validation Oracle for hyir v1 (Binary IR & Materializer).
Answering Proposal #4700: "[wildcard] Delete the Go-text emitter: checked graph lowers to a binary IR materialized as go/ast"
"""

import struct
from enum import IntEnum
from typing import List, Optional, Tuple

MAGIC_HEADER = b"\x7fHYIR"
SCHEMA_VERSION = 1

class NodeKind(IntEnum):
    PACKAGE = 1
    FUNC_DECL = 2
    TASK_SPAWN = 3
    CHAN_MAKE = 4
    CHAN_SEND = 5
    CHAN_RECV = 6
    CALL = 7
    RETURN = 8
    LITERAL_STRING = 9
    LITERAL_INT = 10

class EffectCertificate(IntEnum):
    NONE = 0
    AFFINE = 1 << 0
    SENDABLE = 1 << 1
    LINEAR = 1 << 2
    EXCLUSIVE = 1 << 3

class IRNode:
    def __init__(
        self,
        kind: NodeKind,
        name: str = "",
        type_str: str = "",
        value_str: str = "",
        value_int: int = 0,
        span: Tuple[int, int] = (0, 0),
        certificate: EffectCertificate = EffectCertificate.NONE,
        children: Optional[List["IRNode"]] = None
    ):
        self.kind = kind
        self.name = name
        self.type_str = type_str
        self.value_str = value_str
        self.value_int = value_int
        self.span = span
        self.certificate = certificate
        self.children = children or []

    def to_binary(self) -> bytes:
        parts = []
        parts.append(bytes([int(self.kind)]))
        parts.append(struct.pack(">II", self.span[0], self.span[1]))
        parts.append(struct.pack(">H", int(self.certificate)))

        name_b = self.name.encode("utf-8")
        parts.append(struct.pack(">H", len(name_b)) + name_b)

        type_b = self.type_str.encode("utf-8")
        parts.append(struct.pack(">H", len(type_b)) + type_b)

        val_b = self.value_str.encode("utf-8")
        parts.append(struct.pack(">H", len(val_b)) + val_b)

        parts.append(struct.pack(">q", self.value_int))
        parts.append(struct.pack(">H", len(self.children)))

        for child in self.children:
            parts.append(child.to_binary())

        return b"".join(parts)

    @classmethod
    def from_binary(cls, data: bytes, offset: int = 0) -> Tuple["IRNode", int]:
        kind = NodeKind(data[offset])
        offset += 1

        span_start, span_end = struct.unpack_from(">II", data, offset)
        offset += 8

        cert = EffectCertificate(struct.unpack_from(">H", data, offset)[0])
        offset += 2

        name_len = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        name = data[offset : offset + name_len].decode("utf-8")
        offset += name_len

        type_len = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        type_str = data[offset : offset + type_len].decode("utf-8")
        offset += type_len

        val_len = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        val_str = data[offset : offset + val_len].decode("utf-8")
        offset += val_len

        val_int = struct.unpack_from(">q", data, offset)[0]
        offset += 8

        num_children = struct.unpack_from(">H", data, offset)[0]
        offset += 2

        children = []
        for _ in range(num_children):
            child, offset = cls.from_binary(data, offset)
            children.append(child)

        return (
            cls(
                kind=kind,
                name=name,
                type_str=type_str,
                value_str=val_str,
                value_int=val_int,
                span=(span_start, span_end),
                certificate=cert,
                children=children,
            ),
            offset,
        )

def encode_hyir(root: IRNode) -> bytes:
    header = MAGIC_HEADER + bytes([SCHEMA_VERSION])
    return header + root.to_binary()

def decode_hyir(data: bytes) -> IRNode:
    if len(data) < 6:
        raise ValueError("HYB-IR-E04: Truncated binary stream")
    if data[:5] != MAGIC_HEADER:
        raise ValueError("HYB-IR-E02: Invalid magic header")
    if data[5] != SCHEMA_VERSION:
        raise ValueError(f"HYB-IR-E03: Unsupported schema version {data[5]}")
    node, _ = IRNode.from_binary(data, 6)
    return node

def test_hyir_properties():
    # Build typed task node with U+FEFF BOM and backticks
    root = IRNode(
        kind=NodeKind.PACKAGE,
        name="worker_pkg",
        children=[
            IRNode(
                kind=NodeKind.FUNC_DECL,
                name="ExecuteTask",
                certificate=EffectCertificate.AFFINE | EffectCertificate.SENDABLE,
                children=[
                    IRNode(
                        kind=NodeKind.LITERAL_STRING,
                        name="bom_str",
                        value_str="\ufeffHello `World` with %s format",
                    ),
                    IRNode(
                        kind=NodeKind.TASK_SPAWN,
                        name="child_worker",
                        certificate=EffectCertificate.SENDABLE,
                    ),
                    IRNode(
                        kind=NodeKind.RETURN,
                        name="success",
                        value_int=1,
                    )
                ]
            )
        ]
    )

    encoded = encode_hyir(root)
    decoded = decode_hyir(encoded)

    assert decoded.name == "worker_pkg"
    fn = decoded.children[0]
    assert fn.name == "ExecuteTask"
    assert fn.certificate == (EffectCertificate.AFFINE | EffectCertificate.SENDABLE)

    str_literal = fn.children[0]
    # Proof 1: BOM and backticks preserved bit-for-bit without corruption
    assert str_literal.value_str == "\ufeffHello `World` with %s format"
    print("Proof 1: Verbatim data preservation verified (U+FEFF and backticks preserved).")

    # Proof 2: Task spawn lowers without string parsing
    task_node = fn.children[1]
    assert task_node.kind == NodeKind.TASK_SPAWN
    assert task_node.name == "child_worker"
    print("Proof 2: Task spawn node serialized without string emitter round-trip.")

    # Size comparison:
    equivalent_go_text = """package worker_pkg

func ExecuteTask() int {
    bom_str := "\ufeffHello `World` with %s format"
    go child_worker()
    return 1
}
"""
    text_size = len(equivalent_go_text.encode("utf-8"))
    binary_size = len(encoded)
    print(f"Go Source Text Size: {text_size} bytes | hyir Binary Size: {binary_size} bytes")
    print("ALL MODEL PROOFS PASSED.")

if __name__ == "__main__":
    test_hyir_properties()
