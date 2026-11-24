#!/usr/bin/env python3
import unittest
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from hyir_model import (
    NodeKind, EffectCertificate, IRNode, encode_hyir, decode_hyir,
    MAGIC_HEADER, SCHEMA_VERSION
)

class TestHyirBinary(unittest.TestCase):
    def test_header_and_version(self):
        root = IRNode(kind=NodeKind.PACKAGE, name="pkg")
        data = encode_hyir(root)
        self.assertEqual(data[:5], MAGIC_HEADER)
        self.assertEqual(data[5], SCHEMA_VERSION)

    def test_roundtrip(self):
        root = IRNode(
            kind=NodeKind.PACKAGE,
            name="worker",
            children=[
                IRNode(
                    kind=NodeKind.FUNC_DECL,
                    name="Run",
                    certificate=EffectCertificate.AFFINE | EffectCertificate.SENDABLE,
                    children=[
                        IRNode(kind=NodeKind.TASK_SPAWN, name="child_fn"),
                        IRNode(kind=NodeKind.RETURN, name="", value_int=100)
                    ]
                )
            ]
        )
        data = encode_hyir(root)
        decoded = decode_hyir(data)
        self.assertEqual(decoded.name, "worker")
        self.assertEqual(len(decoded.children), 1)
        fn = decoded.children[0]
        self.assertEqual(fn.name, "Run")
        self.assertEqual(fn.certificate, EffectCertificate.AFFINE | EffectCertificate.SENDABLE)
        self.assertEqual(len(fn.children), 2)
        self.assertEqual(fn.children[0].kind, NodeKind.TASK_SPAWN)
        self.assertEqual(fn.children[1].value_int, 100)

    def test_escaping_immunity(self):
        val = "\ufeff`backtick` and %s formatting"
        node = IRNode(kind=NodeKind.LITERAL_STRING, value_str=val)
        root = IRNode(kind=NodeKind.PACKAGE, name="p", children=[node])
        data = encode_hyir(root)
        decoded = decode_hyir(data)
        self.assertEqual(decoded.children[0].value_str, val)

    def test_invalid_header_rejected(self):
        with self.assertRaises(ValueError):
            decode_hyir(b"\x00\x00\x00\x00\x00\x01")

if __name__ == "__main__":
    unittest.main()
