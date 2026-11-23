"""W-4700: Loader materializing binary hyir v0 directly into go/ast representations."""

from __future__ import annotations
from dataclasses import dataclass, field
from typing import List, Optional
from hyir import IRModule, IRNode, NodeKind, TypeKind, deserialize_hyir


@dataclass
class GoIdent:
    name: str


@dataclass
class GoBasicLit:
    kind: str  # "INT", "STRING"
    value: str


@dataclass
class GoCallExpr:
    fun: str
    args: List[str] = field(default_factory=list)


@dataclass
class GoSendStmt:
    chan_name: str
    value: str


@dataclass
class GoRecvStmt:
    lhs: str
    chan_name: str


@dataclass
class GoFuncDecl:
    name: str
    body: List[any] = field(default_factory=list)
    span_comment: str = ""


@dataclass
class GoFile:
    package_name: str
    decls: List[GoFuncDecl] = field(default_factory=list)
    effect_certificates: List[str] = field(default_factory=list)


class MaterializationError(Exception):
    pass


class ASTMaterializer:
    """Materializes a binary hyir v0 payload into go/ast structures without intermediate text emission."""

    def materialize_from_bytes(self, payload: bytes) -> GoFile:
        module = deserialize_hyir(payload)
        return self.materialize_module(module)

    def materialize_module(self, module: IRModule) -> GoFile:
        gofile = GoFile(package_name=module.package_name)

        for node in module.nodes:
            if node.kind in (NodeKind.FUNC_DECL, NodeKind.TASK_DECL):
                func_decl = self._materialize_func(node, gofile)
                gofile.decls.append(func_decl)
            else:
                raise MaterializationError(f"Unsupported top-level node kind: {node.kind}")

        return gofile

    def _materialize_func(self, node: IRNode, gofile: GoFile) -> GoFuncDecl:
        span_str = f"//line file_{node.span.file_id}.hy:{node.span.line}:{node.span.col}"
        body_stmts = []

        for child in node.children:
            # Validate effect certificate if present
            if child.effect_cert is not None:
                if not child.effect_cert.verified:
                    raise MaterializationError(
                        f"HYB-E6001: Uncertified effect node in hyir payload: {child.name} lacks valid certificate"
                    )
                gofile.effect_certificates.append(f"{child.name}:{child.effect_cert.cert_id}")

            if child.kind == NodeKind.CALL_EXPR:
                body_stmts.append(GoCallExpr(fun=child.name, args=[]))
            elif child.kind == NodeKind.CHAN_SEND:
                val = child.string_payload if child.string_payload is not None else "0"
                body_stmts.append(GoSendStmt(chan_name=child.name, value=val))
            elif child.kind == NodeKind.CHAN_RECV:
                body_stmts.append(GoRecvStmt(lhs="res", chan_name=child.name))
            elif child.kind == NodeKind.VAR_DECL:
                val = child.string_payload if child.string_payload is not None else "0"
                body_stmts.append(f"{child.name} := {val}")

        return GoFuncDecl(
            name=node.name,
            body=body_stmts,
            span_comment=span_str,
        )
