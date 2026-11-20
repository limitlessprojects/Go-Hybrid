#!/usr/bin/env python3
"""Unit tests for hy-ffi-cert (spike for #610).

Run from the repository root:

    python3 -m unittest spikes/ffi-cert-610/test_hy_ffi_cert.py -v

The contract tests need `spikes/ffi-cert-610/../target/release/libhyrt.a`,
i.e. `hy_ffi_cert.py build`; they skip cleanly when it is absent so the suite
stays runnable on a machine without a Rust toolchain.
"""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import hy_ffi_cert as h  # noqa: E402


class ParserTests(unittest.TestCase):
    def test_uleb128_roundtrip(self):
        self.assertEqual(h.uleb128(b"\x00", 0), (0, 1))
        self.assertEqual(h.uleb128(b"\x7f", 0), (127, 1))
        self.assertEqual(h.uleb128(b"\x80\x01", 0), (128, 2))
        self.assertEqual(h.uleb128(b"\xe5\x8e\x26", 0), (624_485, 3))  # LLVM doc example

    def test_canonical_digest_is_key_order_independent(self):
        a = {"b": 1, "a": [1, {"z": 2, "y": 3}]}
        b = {"a": [1, {"y": 3, "z": 2}], "b": 1}
        self.assertEqual(h.digest(a), h.digest(b))
        self.assertNotEqual(h.digest(a), h.digest({**a, "b": 2}))

    def test_canonical_bytes_are_stable(self):
        self.assertEqual(h.canonical({"x": 1, "y": None}), b'{"x":1,"y":null}')

    def test_data_slot_vs_function_symbol(self):
        self.assertTrue(h._is_data_slot(".data.rel.ro..Lanon.8034cc74746c0f70.74"))
        self.assertTrue(h._is_data_slot(".data._RNvCsb42QRNgl0BG_4hyrt5TABLE"))
        self.assertFalse(h._is_data_slot("_RNvNtNtCsgHYyB3hU8Ec_3std9panicking9panic_fmt"))

    def test_split_args_handles_nested_calls(self):
        args = "FnParse, dptr, uintptr(len(data)), uintptr(unsafe.Pointer(&id))"
        self.assertEqual(len(h._split_args(args)), 4)
        self.assertEqual(h._split_args(""), [])

    def test_balanced_args_stops_at_the_matching_paren(self):
        text = "hybridge.CrossDeep3(FnParse, ptr, n, uintptr(unsafe.Pointer(&id))) + tail(1)"
        start = text.index("FnParse") + len("FnParse")
        args = h._balanced_args(text, start)
        self.assertEqual(len(h._split_args(args)), 3)   # FnParse itself is not an argument
        self.assertIn("unsafe.Pointer(&id)", args)

    def test_fn_from_section(self):
        self.assertEqual(h._fn_from_section(".text.hy_json_parse"), "hy_json_parse")
        self.assertEqual(h._fn_from_section(".text.unlikely._RNvX_foo"), "_RNvX_foo")
        self.assertEqual(h._fn_from_section(".data.rel.ro"), "")

    def test_wrapper_subset_flags_leaks_and_blocking(self):
        src = Path(tempfile.mkdtemp()) / "w.rs"
        src.write_text(
            '#[no_mangle]\npub extern "C" fn hy_leak(p: *const u8, n: u64) -> i32 {\n'
            '    let s = unsafe { std::slice::from_raw_parts(p, n as usize) };\n'
            '    let l: &\'static [u8] = Box::leak(s.to_vec().into_boxed_slice());\n'
            '    l.len() as i32\n}\n\n'
            '#[no_mangle]\npub extern "C" fn hy_block(x: u64) -> u64 {\n'
            '    let g = std::sync::Mutex::new(0u64);\n'
            '    let _l = g.lock().unwrap();\n'
            '    x\n}\n',
            encoding="utf-8",
        )
        eff = h.wrapper_effects(src)
        self.assertTrue(eff["hy_leak"]["may_retain_ptr"])
        self.assertTrue(eff["hy_block"]["blocking"])
        self.assertFalse(eff["hy_leak"]["blocking"])


@unittest.skipUnless(h.default_artifact().exists(),
                     "libhyrt.a not built — run `hy_ffi_cert.py build` first")
class ContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contract = h.build_contract(
            h.default_artifact(), h.repo_root() / "runtime/go",
            h.repo_root() / "runtime/rust/hyrt/src/lib.rs", "test")
        cls.tmp = Path(tempfile.mkdtemp(prefix="hyffi-test-"))

    def test_every_export_has_a_signature(self):
        missing = [s for s, f in self.contract["symbols"].items()
                   if f["abi"]["arity"] is None]
        self.assertEqual(missing, [])
        self.assertGreaterEqual(len(self.contract["symbols"]), 20)

    def test_stack_records_are_real_numbers(self):
        facts = self.contract["symbols"]["hy_sha3_digest256"]["stack"]
        self.assertGreater(facts["bytes"], 0)
        self.assertIn(facts["class"], ("SMALL", "DEFAULT", "DEEP"))

    def test_no_export_is_certified_faster_than_its_facts(self):
        # fail-closed: an escalated export must never be labelled SMALL
        for sym, fact in self.contract["symbols"].items():
            st = fact["stack"]
            if st["recursive"] or st["indirect_call"] or st["missing_frames"]:
                self.assertEqual(st["class"], "DEEP", sym)

    def test_six_fixtures_are_all_rejected(self):
        res = h.run_fixtures(self.contract, h.default_artifact(),
                             h.repo_root() / "runtime/go", self.tmp)
        self.assertEqual([r["fixture"] for r in res],
                         ["f1_signature_drift", "f2_recursion_as_small",
                          "f3_retained_borrow", "f4_callback_edge",
                          "f5_blocking_beneath_nonblocking", "f6_hash_mismatch"])
        self.assertEqual(sum(1 for r in res if r["rejected"]), 6)

    def test_contract_digest_matches_its_own_payload(self):
        body = {k: v for k, v in self.contract.items() if k != "contract_sha256"}
        self.assertEqual(h.digest(body), self.contract["contract_sha256"])

    def test_serialised_contract_is_valid_json(self):
        again = json.loads(json.dumps(self.contract, sort_keys=True))
        self.assertEqual(again["contract_sha256"], self.contract["contract_sha256"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
