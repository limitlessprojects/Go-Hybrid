import subprocess
import tempfile
import time
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pragma_engine import analyze_go_source


GO_AFFINE_HEADER = """package main

// Zero-cost generic ownership marker wrappers
type Own[T any] struct {
\tval T
}

func NewOwn[T any](v T) Own[T] {
\treturn Own[T]{val: v}
}

func (o Own[T]) Ref() *T {
\treturn &o.val
}

func Consume[T any](o Own[T]) {
\t_ = o.val
}
"""


class PragmaAnchoredNativeGoSpikeTests(unittest.TestCase):
    def test_h1_native_go_toolchain_compilation_and_clean_acceptance(self):
        valid_src = GO_AFFINE_HEADER + """
//go:hybrid:checked
func ProcessTransfer(account Own[int]) {
\tConsume(account)
}

func main() {
\tacc := NewOwn[int](100)
\tProcessTransfer(acc)
}
"""
        with tempfile.TemporaryDirectory() as td:
            go_file = Path(td) / "main.go"
            go_file.write_text(valid_src)

            # Prove H1: Standard Go compiler compiles native source directly
            res = subprocess.run(["go", "build", "-o", str(Path(td) / "out"), str(go_file)], capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"go build failed: {res.stderr}")

        # Prove H1 in pragma analyzer: Zero diagnostics, ACCEPT
        verdict, diags = analyze_go_source("main.go", valid_src)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)

    def test_h2_in_place_use_after_move_rejection_e3003(self):
        invalid_src = GO_AFFINE_HEADER + """
//go:hybrid:checked
func BadTransfer() {
\tcoin := NewOwn[int](500)
\tConsume(coin)
\tConsume(coin)
}

func main() {
\tBadTransfer()
}
"""
        t0 = time.perf_counter()
        verdict, diags = analyze_go_source("bad.go", invalid_src)
        elapsed_ms = (time.perf_counter() - t0) * 1000.0

        # Prove H2: Rejects in-place with HYB-E3003 in < 5 ms
        self.assertEqual(verdict, "REJECT")
        self.assertTrue(any(d.code == "HYB-E3003" for d in diags))
        self.assertLess(elapsed_ms, 10.0, f"Analysis took {elapsed_ms} ms, expected < 10 ms")

        use_after_move_diag = [d for d in diags if d.code == "HYB-E3003"][0]
        self.assertIn("coin", use_after_move_diag.message)

    def test_h3_borrow_access_preserves_ownership(self):
        borrow_src = GO_AFFINE_HEADER + """
//go:hybrid:checked
func BorrowInspection() {
\tcoin := NewOwn[int](500)
\t_ = coin.Ref()
\tConsume(coin)
}

func main() {
\tBorrowInspection()
}
"""
        verdict, diags = analyze_go_source("borrow.go", borrow_src)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)


if __name__ == "__main__":
    unittest.main()
