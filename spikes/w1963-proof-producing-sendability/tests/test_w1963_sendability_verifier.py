"""Unit tests for W-1963 (#1963): Independent Proof-Carrying Sendability Verifier."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from sendability_verifier import (  # noqa: E402
     SEMANTICS_VERSION,
    BindingDecl,
    SendabilityCertificate,
    compute_digest,
    verify_sendability_proof,
)


class TestW1963ProofProducingSendability(unittest.TestCase):
    def setUp(self):
        self.src = "fn run() { p := Point{x: 1, y: 2}; task.spawn(worker, p) }"
        self.imp = "struct Point { x: int, y: int }"
        self.src_hash = compute_digest(self.src)
        self.imp_hash = compute_digest(self.imp)

    def test_01_valid_primitive_and_value_struct_derivations_certified(self):
        decls = [BindingDecl("b_p", "p", "Point", 0), BindingDecl("b_n", "n", "int", 0)]
        structs = {"Point": {"x": "int", "y": "int"}}
        c1 = SendabilityCertificate(SEMANTICS_VERSION, self.src_hash, self.imp_hash, "n", "b_n", "RULE_PRIM_COPY")
        self.assertTrue(verify_sendability_proof(self.src, self.imp, decls, structs, c1).certified)

        c2 = SendabilityCertificate(
            SEMANTICS_VERSION, self.src_hash, self.imp_hash, "p", "b_p", "RULE_VALUE_STRUCT_CLOSURE",
            claimed_struct_fields=(("x", "int"), ("y", "int")),
        )
        self.assertTrue(verify_sendability_proof(self.src, self.imp, decls, structs, c2).certified)

    def test_02_forged_primitive_claim_on_slice_rejected(self):
        decls = [BindingDecl("b_s", "s", "[]int", 0)]
        c = SendabilityCertificate(SEMANTICS_VERSION, self.src_hash, self.imp_hash, "s", "b_s", "RULE_PRIM_COPY")
        verdict = verify_sendability_proof(self.src, self.imp, decls, {}, c)
        self.assertFalse(verdict.certified)
        self.assertEqual(verdict.code, "W1963_ERR_FORGED_PRIMITIVE")

    def test_03_shadowed_binding_id_substitution_rejected(self):
        decls = [
            BindingDecl("b_outer_int", "x", "int", scope_depth=0),
            BindingDecl("b_inner_slice", "x", "[]int", scope_depth=1),
        ]
        c = SendabilityCertificate(SEMANTICS_VERSION, self.src_hash, self.imp_hash, "x", "b_outer_int", "RULE_PRIM_COPY")
        verdict = verify_sendability_proof(self.src, self.imp, decls, {}, c)
        self.assertFalse(verdict.certified)
        self.assertEqual(verdict.code, "W1963_ERR_SHADOWED_BINDING_ID")

    def test_04_omitted_pointer_field_in_struct_closure_rejected(self):
        decls = [BindingDecl("b_pkt", "pkt", "Packet", 0)]
        structs = {"Packet": {"id": "int", "buf": "[]byte"}}
        c = SendabilityCertificate(
            SEMANTICS_VERSION, self.src_hash, self.imp_hash, "pkt", "b_pkt", "RULE_VALUE_STRUCT_CLOSURE",
            claimed_struct_fields=(("id", "int"),),
        )
        verdict = verify_sendability_proof(self.src, self.imp, decls, structs, c)
        self.assertFalse(verdict.certified)
        self.assertEqual(verdict.code, "W1963_ERR_OMITTED_STRUCT_FIELD")

    def test_05_tampered_source_hash_and_post_move_use_rejected(self):
        decls = [BindingDecl("b_buf", "buf", "[]byte", 0, used_after_spawn=True)]
        c_hash = SendabilityCertificate(SEMANTICS_VERSION, "bad_hash", self.imp_hash, "buf", "b_buf", "RULE_UNIQUE_FRESH_MOVE", has_explicit_move=True)
        self.assertEqual(verify_sendability_proof(self.src, self.imp, decls, {}, c_hash).code, "W1963_ERR_SOURCE_DIGEST_MISMATCH")

        c_uaf = SendabilityCertificate(SEMANTICS_VERSION, self.src_hash, self.imp_hash, "buf", "b_buf", "RULE_UNIQUE_FRESH_MOVE", has_explicit_move=True)
        self.assertEqual(verify_sendability_proof(self.src, self.imp, decls, {}, c_uaf).code, "W1963_ERR_LINEARITY_VIOLATION")


if __name__ == "__main__":
    unittest.main()
