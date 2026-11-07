import unittest
import sys
import pathlib

SPIKE_DIR = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SPIKE_DIR))

import teardown_verifier


class TestNurseryTeardownVerifier(unittest.TestCase):
    def test_hy_cases_all_pass(self):
        all_ok, results = teardown_verifier.verify_hy_test_cases()
        for name, res in results.items():
            self.assertTrue(
                res["matched"],
                f"File {name} failed: expected {res['expected']}, got {res['actual']}"
            )
        self.assertTrue(all_ok)

    def test_admitted_case(self):
        code = """
        fn worker(ctx context, in_ch rx[int], w CancelWitness) TeardownResult {
            select {
            case <-ctx.done():
                for _ in in_ch.drain() {}
                return w.abort()
            }
        }
        """
        verdict, diags = teardown_verifier.analyze_hy_source(code)
        self.assertEqual(verdict, "ACCEPT")
        self.assertEqual(len(diags), 0)

    def test_rejected_undrained_case(self):
        code = """
        fn leaky(ctx context, in_ch rx[int], w CancelWitness) TeardownResult {
            select {
            case <-ctx.done():
                return w.abort()
            }
        }
        """
        verdict, diags = teardown_verifier.analyze_hy_source(code)
        self.assertIn("HYB-E3910", verdict)
        self.assertEqual(len(diags), 1)

    def test_rejected_bare_go_escape(self):
        code = """
        fn rogue(ctx context, w CancelWitness) TeardownResult {
            go fn() {
                time.sleep(100)
            }()
            return w.complete()
        }
        """
        verdict, diags = teardown_verifier.analyze_hy_source(code)
        self.assertIn("HYB-E3911", verdict)
        self.assertEqual(len(diags), 1)

    def test_falsifiers(self):
        f1, f2 = teardown_verifier.verify_falsifiers_f1_f2()
        self.assertTrue(f1)
        self.assertTrue(f2)


if __name__ == "__main__":
    unittest.main()
