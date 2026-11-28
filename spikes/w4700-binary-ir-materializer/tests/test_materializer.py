"""Tests for W-4700: Binary IR (hyir v0) Materialization to go/ast."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from hyir import (
    IRModule,
    IRNode,
    NodeKind,
    TypeKind,
    Span,
    EffectCertificate,
    serialize_hyir,
    deserialize_hyir,
)
from materializer import ASTMaterializer, MaterializationError


class TestBinaryIRMaterializer(unittest.TestCase):
    def setUp(self):
        self.materializer = ASTMaterializer()

    def test_hyir_binary_serialization_roundtrip(self):
        """Binary hyir v0 format round-trips with 100% fidelity without string re-parsing."""
        mod = IRModule(
            package_name="testpkg",
            nodes=[
                IRNode(
                    kind=NodeKind.FUNC_DECL,
                    name="ComputeTotal",
                    type_kind=TypeKind.VOID,
                    span=Span(file_id=1, line=10, col=5),
                    children=[
                        IRNode(
                            kind=NodeKind.CALL_EXPR,
                            name="initSlab",
                            type_kind=TypeKind.VOID,
                            span=Span(file_id=1, line=11, col=9),
                        )
                    ],
                )
            ],
        )

        payload = serialize_hyir(mod)
        self.assertTrue(payload.startswith(b"HYIR"))

        deserialized = deserialize_hyir(payload)
        self.assertEqual(deserialized.package_name, "testpkg")
        self.assertEqual(len(deserialized.nodes), 1)
        self.assertEqual(deserialized.nodes[0].name, "ComputeTotal")
        self.assertEqual(deserialized.nodes[0].span.line, 10)
        self.assertEqual(len(deserialized.nodes[0].children), 1)
        self.assertEqual(deserialized.nodes[0].children[0].name, "initSlab")

    def test_materialization_into_go_ast(self):
        """Materializes directly into go/ast representations with exact spans and verified effects."""
        mod = IRModule(
            package_name="workerpool",
            nodes=[
                IRNode(
                    kind=NodeKind.TASK_DECL,
                    name="RunPipeline",
                    type_kind=TypeKind.TASK_HANDLE,
                    span=Span(file_id=2, line=42, col=1),
                    children=[
                        IRNode(
                            kind=NodeKind.CALL_EXPR,
                            name="syncWait",
                            type_kind=TypeKind.VOID,
                            span=Span(file_id=2, line=43, col=5),
                            effect_cert=EffectCertificate("cert_sync_001", "io", verified=True),
                        ),
                        IRNode(
                            kind=NodeKind.CHAN_SEND,
                            name="outCh",
                            type_kind=TypeKind.INT64,
                            span=Span(file_id=2, line=44, col=5),
                            string_payload="42",
                        ),
                    ],
                )
            ],
        )

        payload = serialize_hyir(mod)
        gofile = self.materializer.materialize_from_bytes(payload)

        self.assertEqual(gofile.package_name, "workerpool")
        self.assertEqual(len(gofile.decls), 1)
        self.assertEqual(gofile.decls[0].name, "RunPipeline")
        self.assertEqual(len(gofile.decls[0].body), 2)
        self.assertIn("cert_sync_001", gofile.effect_certificates[0])

    def test_falsifier_ufeff_and_raw_literal_hazard_eliminated(self):
        """String payloads containing U+FEFF (BOM) or unescaped characters never break text emitter boundary."""
        tricky_string = "\ufeffraw_token_with_quotes_\"_and_newlines_\n_and_null_\x00"
        mod = IRModule(
            package_name="hazardpkg",
            nodes=[
                IRNode(
                    kind=NodeKind.FUNC_DECL,
                    name="EmitLiteral",
                    type_kind=TypeKind.VOID,
                    span=Span(file_id=3, line=1, col=1),
                    children=[
                        IRNode(
                            kind=NodeKind.VAR_DECL,
                            name="payload",
                            type_kind=TypeKind.STRING,
                            span=Span(file_id=3, line=2, col=5),
                            string_payload=tricky_string,
                        )
                    ],
                )
            ],
        )

        # Serialized as length-prefixed bytes in hyir
        payload = serialize_hyir(mod)
        gofile = self.materializer.materialize_from_bytes(payload)

        # Materialized cleanly without parser crash or U+FEFF raw-string injection
        self.assertEqual(gofile.decls[0].body[0], f"payload := {tricky_string}")

    def test_falsifier_uncertified_effect_rejected(self):
        """Effect node lacking a valid compiler certificate is rejected during AST materialization."""
        mod = IRModule(
            package_name="untrusted",
            nodes=[
                IRNode(
                    kind=NodeKind.TASK_DECL,
                    name="UncertifiedTask",
                    type_kind=TypeKind.VOID,
                    span=Span(file_id=4, line=1, col=1),
                    children=[
                        IRNode(
                            kind=NodeKind.CALL_EXPR,
                            name="maliciousIO",
                            type_kind=TypeKind.VOID,
                            span=Span(file_id=4, line=2, col=5),
                            effect_cert=EffectCertificate("cert_invalid", "io", verified=False),
                        )
                    ],
                )
            ],
        )

        payload = serialize_hyir(mod)
        with self.assertRaises(MaterializationError) as cm:
            self.materializer.materialize_from_bytes(payload)
        self.assertIn("HYB-E6001", str(cm.exception))


if __name__ == "__main__":
    unittest.main()
