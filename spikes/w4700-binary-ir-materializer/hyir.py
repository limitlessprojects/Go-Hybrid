"""W-4700 (#4700): Binary IR (hyir v0) schema and serialization model.

Defines the typed binary intermediate representation replacing Go-text emission.
"""

from __future__ import annotations
import struct
from dataclasses import dataclass, field
from enum import IntEnum
from typing import List, Optional, Tuple


MAGIC = b"HYIR"
VERSION_V0 = 1


class NodeKind(IntEnum):
    PACKAGE = 1
    FUNC_DECL = 2
    TASK_DECL = 3
    VAR_DECL = 4
    CALL_EXPR = 5
    CHAN_SEND = 6
    CHAN_RECV = 7
    EFFECT_CERT = 8


class TypeKind(IntEnum):
    INT64 = 1
    STRING = 2
    BOOL = 3
    TASK_HANDLE = 4
    CHAN = 5
    VOID = 6


@dataclass(frozen=True)
class Span:
    file_id: int
    line: int
    col: int


@dataclass
class EffectCertificate:
    cert_id: str
    effect_kind: str  # "io", "spawn", "net"
    verified: bool = True


@dataclass
class IRNode:
    kind: NodeKind
    name: str
    type_kind: TypeKind
    span: Span
    effect_cert: Optional[EffectCertificate] = None
    children: List["IRNode"] = field(default_factory=list)
    string_payload: Optional[str] = None


@dataclass
class IRModule:
    package_name: str
    nodes: List[IRNode] = field(default_factory=list)


def serialize_hyir(module: IRModule) -> bytes:
    """Serializes IRModule into compact binary hyir v0 format."""
    buf = bytearray()
    # 1. Header: MAGIC (4 bytes) + VERSION (2 bytes)
    buf.extend(MAGIC)
    buf.extend(struct.pack(">H", VERSION_V0))

    # 2. Package name (1 byte length + UTF-8 payload)
    pkg_bytes = module.package_name.encode("utf-8")
    buf.append(len(pkg_bytes))
    buf.extend(pkg_bytes)

    # 3. Node count (4 bytes)
    buf.extend(struct.pack(">I", len(module.nodes)))

    # 4. Serialize nodes
    for node in module.nodes:
        _serialize_node(node, buf)

    return bytes(buf)


def _serialize_node(node: IRNode, buf: bytearray):
    buf.append(node.kind)
    buf.append(node.type_kind)

    # Span (file_id: 2 bytes, line: 4 bytes, col: 2 bytes)
    buf.extend(struct.pack(">HIH", node.span.file_id, node.span.line, node.span.col))

    # Name
    name_bytes = node.name.encode("utf-8")
    buf.append(len(name_bytes))
    buf.extend(name_bytes)

    # Effect certificate flag
    if node.effect_cert is not None:
        buf.append(1 if node.effect_cert.verified else 2) # 1: verified, 2: unverified
        cid_bytes = node.effect_cert.cert_id.encode("utf-8")
        buf.append(len(cid_bytes))
        buf.extend(cid_bytes)
    else:
        buf.append(0)

    # String payload (e.g. raw string literal value without text escaping errors)
    if node.string_payload is not None:
        buf.append(1)
        sp_bytes = node.string_payload.encode("utf-8")
        buf.extend(struct.pack(">I", len(sp_bytes)))
        buf.extend(sp_bytes)
    else:
        buf.append(0)

    # Children
    buf.extend(struct.pack(">H", len(node.children)))
    for child in node.children:
        _serialize_node(child, buf)


def deserialize_hyir(data: bytes) -> IRModule:
    """Deserializes binary hyir v0 format back into IRModule."""
    if len(data) < 6:
        raise ValueError("Payload too short for hyir header")
    magic = data[:4]
    if magic != MAGIC:
        raise ValueError(f"Invalid magic bytes: {magic}")
    version, = struct.unpack(">H", data[4:6])
    if version != VERSION_V0:
        raise ValueError(f"Unsupported hyir version: {version}")

    offset = 6
    pkg_len = data[offset]
    offset += 1
    pkg_name = data[offset:offset+pkg_len].decode("utf-8")
    offset += pkg_len

    node_count, = struct.unpack(">I", data[offset:offset+4])
    offset += 4

    nodes: List[IRNode] = []
    for _ in range(node_count):
        node, offset = _deserialize_node(data, offset)
        nodes.append(node)

    return IRModule(package_name=pkg_name, nodes=nodes)


def _deserialize_node(data: bytes, offset: int) -> Tuple[IRNode, int]:
    kind = NodeKind(data[offset])
    type_kind = TypeKind(data[offset+1])
    offset += 2

    file_id, line, col = struct.unpack(">HIH", data[offset:offset+8])
    offset += 8
    span = Span(file_id, line, col)

    name_len = data[offset]
    offset += 1
    name = data[offset:offset+name_len].decode("utf-8")
    offset += name_len

    cert_flag = data[offset]
    offset += 1
    effect_cert = None
    if cert_flag in (1, 2):
        cid_len = data[offset]
        offset += 1
        cid = data[offset:offset+cid_len].decode("utf-8")
        offset += cid_len
        effect_cert = EffectCertificate(cert_id=cid, effect_kind="io", verified=(cert_flag == 1))

    has_payload = data[offset]
    offset += 1
    string_payload = None
    if has_payload == 1:
        sp_len, = struct.unpack(">I", data[offset:offset+4])
        offset += 4
        string_payload = data[offset:offset+sp_len].decode("utf-8")
        offset += sp_len

    child_count, = struct.unpack(">H", data[offset:offset+2])
    offset += 2
    children: List[IRNode] = []
    for _ in range(child_count):
        cnode, offset = _deserialize_node(data, offset)
        children.append(cnode)

    return IRNode(
        kind=kind,
        name=name,
        type_kind=type_kind,
        span=span,
        effect_cert=effect_cert,
        children=children,
        string_payload=string_payload,
    ), offset
