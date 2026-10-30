#!/usr/bin/env python3
"""Unit tests for hy-ffi-cert: Proof-carrying FFI contract verifier (#610)."""
from __future__ import annotations
import tempfile, unittest
from pathlib import Path
import hy_ffi_cert as cert

class TestHyFfiCert(unittest.TestCase):
    def test_uleb128(self):
        self.assertEqual(cert.uleb128(b"\x00", 0), (0, 1))
        self.assertEqual(cert.uleb128(b"\x7f", 0), (127, 1))
        self.assertEqual(cert.uleb128(b"\x80\x01", 0), (128, 2))
        self.assertEqual(cert.uleb128(b"\xe5\x8e\x26", 0), (624485, 3))

    def test_canonical_digest(self):
        self.assertEqual(cert.digest({"b": 2, "a": [1, 2]}), cert.digest({"a": [1, 2], "b": 2}))

    def test_closure_facts_recursion_and_missing_frames(self):
        r1 = cert.closure_facts({"A": ["B"], "B": ["A"]}, {"A": 64, "B": 128}, "A")
        self.assertTrue(r1["recursive"])
        self.assertEqual(r1["class"], "DEEP")

        r2 = cert.closure_facts({"A": ["C"]}, {"A": 64}, "A")
        self.assertIn("C", r2["missing_frames"])
        self.assertEqual(r2["class"], "DEEP")

        r3 = cert.closure_facts({"A": []}, {"A": 256}, "A")
        self.assertFalse(r3["recursive"])
        self.assertEqual(r3["class"], "SMALL")
        self.assertEqual(r3["bytes"], 256)

    def test_wrapper_effects(self):
        with tempfile.NamedTemporaryFile("w+", suffix=".rs", delete=False) as f:
            f.write('#[no_mangle]\npub extern "C" fn hy_test_leak(p: *const u8) -> i32 {\n    let _b = Box::leak(Box::new(10));\n    0\n}\n')
            p = Path(f.name)
        try:
            eff = cert.wrapper_effects(p)
            self.assertTrue(eff["hy_test_leak"]["may_retain_ptr"])
            self.assertFalse(eff["hy_test_leak"]["blocking"])
        finally:
            p.unlink()

    def test_six_negative_fixtures_fail_closed(self):
        repo_go = Path(__file__).resolve().parents[2] / "runtime" / "go"
        externs, _, _ = cert.go_bindings(repo_go)
        self.assertTrue(len(externs) > 0)
        base = {
            "schema": cert.SCHEMA,
            "artifact": {"sha256": "abcdef"},
            "symbols": {
                sym: {
                    "abi": {"arity": e["arity"], "params": e["params"], "ret": e["ret"]},
                    "stack": {"bytes": 128, "class": "SMALL", "recursive": False, "indirect_call": False, "missing_frames": [], "blocking_callees": []},
                    "effects": {"may_retain_ptr": False, "may_callback": False, "blocking": False},
                } for sym, e in externs.items()
            },
        }
        res = cert.run_fixtures(base, repo_go)
        self.assertEqual(len(res), 6)
        for r in res:
            self.assertTrue(r["rejected"], f"Fixture {r['fixture']} not rejected fail-closed")

if __name__ == "__main__":
    unittest.main()
