import unittest
import sys
import pathlib

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SPIKE_DIR))

import channel_epoch_verifier as verifier

class ChannelEpochSpikeTests(unittest.TestCase):
    def test_hy_corpus_verification(self):
        ok, details = verifier.verify_hy_test_cases()
        self.assertTrue(ok, f"HY corpus verification failed: {details}")
        for name, entry in details.items():
            self.assertTrue(entry["matched"], f"Mismatch in {name}: expected {entry['expected']}, got {entry['actual']}")

    def test_clean_affine_accept(self):
        src = """
fn main() {
    tx, rx := make(chan int, 1)
    tx <- 10
    close(move tx)
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)

    def test_double_close_reject(self):
        src = """
fn main() {
    tx, rx := make(chan int, 1)
    close(move tx)
    close(tx)
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3001"))
        self.assertTrue(any("double close" in d for d in diags))

    def test_send_after_close_reject(self):
        src = """
fn main() {
    tx, rx := make(chan int, 1)
    close(move tx)
    tx <- 42
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3001"))
        self.assertTrue(any("post-close send" in d for d in diags))

    def test_split_close_leak_reject(self):
        src = """
fn main() {
    tx, rx := make(chan int, 2)
    tx1, tx2 := tx.split()
    task producer(move tx1)
    close(move tx2)
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3201"))

    def test_circular_wait_reject(self):
        src = """
fn main() {
    chA := make(chan int, 1)
    chB := make(chan int, 1)
    select {
        case <-chB:
            chA <- 1
    }
}
"""
        verdict, diags = verifier.analyze_hy_source(src)
        self.assertTrue(verdict.startswith("REJECT HYB-E3202"))

if __name__ == "__main__":
    unittest.main()
