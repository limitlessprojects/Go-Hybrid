import pathlib
import unittest
from spikes_loader import verify_hy_source, root_dir


class TestLinearObligationVerifier(unittest.TestCase):
    def test_branch_complete_admitted(self):
        src = (root_dir / "hy" / "01-linear-branch-complete-admitted.hy").read_text()
        self.assertEqual(verify_hy_source(src), [])

    def test_early_try_and_partial_branch_rejected(self):
        src = (root_dir / "hy" / "02-linear-early-try-leak-rejected.hy").read_text()
        diags = verify_hy_source(src)
        self.assertEqual(len(diags), 2)
        self.assertTrue(all(d.code == "HYB-E3701" for d in diags))

    def test_noescape_loop_and_falsifier_f2(self):
        src = (root_dir / "hy" / "03-noescape-loop-reuse-admitted.hy").read_text()
        self.assertEqual(verify_hy_source(src), [])

        bad_src = src.replace("pkt.reset()", "sink.Observe(&pkt)")
        diags = verify_hy_source(bad_src)
        self.assertEqual(len(diags), 1)
        self.assertEqual(diags[0].code, "HYB-E3702")


if __name__ == "__main__":
    unittest.main()
