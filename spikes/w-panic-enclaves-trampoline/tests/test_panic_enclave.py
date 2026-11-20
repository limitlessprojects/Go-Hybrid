import unittest
import sys
import pathlib

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SPIKE_DIR))

import panic_enclave_verifier as verifier

class PanicEnclaveSpikeTests(unittest.TestCase):
    def test_hy_corpus_verification(self):
        ok, details = verifier.verify_hy_test_cases()
        self.assertTrue(ok, f"HY corpus verification failed: {details}")
        for name, entry in details.items():
            self.assertTrue(entry["matched"], f"Mismatch in {name}: expected {entry['expected']}, got {entry['actual']}")

    def test_clean_rust_call_accept(self):
        src = """
fn main() {
    h := rust.open_enclave()
    res, err := h.call_safe()
    _ = res
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)

    def test_handled_enclave_accept(self):
        src = """
fn main() {
    h := rust.open_enclave()
    res, err := h.call_panicking_leaf()
    if errors.is(err, hy.err_panic_enclave) {
        fmt.println("handled cleanly")
    }
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)

    def test_poisoned_handle_access_reject(self):
        src = """
fn main() {
    h := rust.open_enclave()
    _, err := h.call_panicking_leaf()
    h.subsequent_call()
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3003"))

    def test_foreign_unmanaged_unwind_reject(self):
        src = """
fn main() {
    foreign.raw_unwind_call()
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3801"))

    def test_poisoned_handle_deref_reject(self):
        src = """
fn main() {
    h := rust.open_enclave()
    _ = h.mutate_and_panic()
    x := *h
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3003"))

if __name__ == "__main__":
    unittest.main()
