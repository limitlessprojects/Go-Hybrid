import unittest
from pathlib import Path
import sys

SPIKE_DIR = Path(__file__).resolve().parent.parent
if str(SPIKE_DIR) not in sys.path:
    sys.path.insert(0, str(SPIKE_DIR))

import pool_scrub_verifier as verifier


class PoolScrubSpikeTests(unittest.TestCase):
    def test_corpus_all_pass(self) -> None:
        report = verifier.verify_corpus()
        self.assertEqual(report["corpus_total"], 6)
        self.assertEqual(report["corpus_passed"], 6)
        self.assertTrue(report["all_passed"])
        self.assertTrue(report["f3_latency_budget_ok"])

    def test_accept_clean_move_scrub_corpus_01(self) -> None:
        src = (verifier.CORPUS_DIR / "01-pool-move-scrub-clean.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        self.assertEqual(diags, [])

    def test_accept_concurrent_task_handoff_corpus_02(self) -> None:
        src = (verifier.CORPUS_DIR / "02-pool-concurrent-task-handoff.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        self.assertEqual(diags, [])

    def test_reject_use_after_put_hyb_e3003(self) -> None:
        src = (verifier.CORPUS_DIR / "03-pool-use-after-put-reject.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        codes = [d["code"] for d in diags]
        self.assertIn("HYB-E3003", codes)

    def test_reject_put_without_move_hyb_e3710(self) -> None:
        src = (verifier.CORPUS_DIR / "04-pool-put-without-move-reject.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        codes = [d["code"] for d in diags]
        self.assertIn("HYB-E3710", codes)

    def test_reject_interior_laundromat_slice_hyb_e3711(self) -> None:
        src = (verifier.CORPUS_DIR / "05-pool-interior-laundromat-slice-reject.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        codes = [d["code"] for d in diags]
        self.assertIn("HYB-E3711", codes)

    def test_reject_subslice_loan_escape_f1_hyb_e3711(self) -> None:
        src = (verifier.CORPUS_DIR / "06-pool-subslice-loan-survives-put-reject.hy").read_text(encoding="utf-8")
        diags = verifier.check_hy_source(src)
        codes = [d["code"] for d in diags]
        self.assertIn("HYB-E3711", codes)

    def test_synthesize_go_scrubber_thunk(self) -> None:
        code = verifier.synthesize_go_scrubber(
            "Packet",
            {
                "ScalarID": "int",
                "Payload": "[]int",
                "Meta": "map[string]int",
                "ForeignPtr": "*int",
            },
        )
        self.assertIn("func __hy_scrub_Packet(p *Packet)", code)
        self.assertIn("p.Payload = p.Payload[:0:cap(p.Payload)]", code)
        self.assertIn("clear(p.Meta)", code)
        self.assertIn("p.ForeignPtr = nil", code)


if __name__ == "__main__":
    unittest.main()
