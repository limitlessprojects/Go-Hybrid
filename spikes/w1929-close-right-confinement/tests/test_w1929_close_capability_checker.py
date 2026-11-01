"""Unit tests for W-1929 (#1929): Close-Right Confinement via Linear CloseRight Capability."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from close_capability_checker import ChanOp, verify_close_confinement  # noqa: E402


class TestW1929CloseRightConfinement(unittest.TestCase):
    def test_01_corpus_15_mpsc_fan_in_accepted_with_post_join_close(self):
        ops = [
            ChanOp("make_chan", "out", "main"),
            ChanOp("spawn_sender", "out", "w1", transfers_close_right=False),
            ChanOp("spawn_sender", "out", "w2", transfers_close_right=False),
            ChanOp("send", "out", "w1"),
            ChanOp("send", "out", "w2"),
            ChanOp("join_all", "out", "main"),
            ChanOp("close", "out", "main"),
        ]
        verdict = verify_close_confinement(ops)
        self.assertTrue(verdict.ok)
        self.assertEqual(verdict.code, "OK")

    def test_02_corpus_16_request_ack_bidirectional_pipeline_accepted(self):
        ops = [
            ChanOp("make_chan", "req", "main"),
            ChanOp("make_chan", "ack", "main"),
            ChanOp("spawn_sender", "ack", "worker", transfers_close_right=True),
            ChanOp("send", "req", "main"),
            ChanOp("close", "req", "main"),
            ChanOp("send", "ack", "worker"),
            ChanOp("close", "ack", "worker"),
        ]
        verdict = verify_close_confinement(ops)
        self.assertTrue(verdict.ok)

    def test_03_duplicate_close_right_holder_and_close_without_right_rejected(self):
        ops_dup = [
            ChanOp("make_chan", "ch", "main"),
            ChanOp("spawn_sender", "ch", "w1", transfers_close_right=True),
            ChanOp("spawn_sender", "ch", "w2", transfers_close_right=True),
        ]
        self.assertEqual(verify_close_confinement(ops_dup).code, "W1929_ERR_DUPLICATE_CLOSE_HOLDER")

        ops_no_right = [
            ChanOp("make_chan", "ch", "main"),
            ChanOp("spawn_sender", "ch", "w1", transfers_close_right=False),
            ChanOp("close", "ch", "w1"),
        ]
        self.assertEqual(verify_close_confinement(ops_no_right).code, "W1929_ERR_CLOSE_WITHOUT_RIGHT")

    def test_04_close_while_senders_live_and_send_after_close_rejected(self):
        ops_live = [
            ChanOp("make_chan", "ch", "main"),
            ChanOp("spawn_sender", "ch", "w1", transfers_close_right=False),
            ChanOp("close", "ch", "main"),
        ]
        self.assertEqual(verify_close_confinement(ops_live).code, "W1929_ERR_CLOSE_WHILE_SENDERS_LIVE")

        ops_after = [
            ChanOp("make_chan", "ch", "main"),
            ChanOp("close", "ch", "main"),
            ChanOp("send", "ch", "main"),
        ]
        self.assertEqual(verify_close_confinement(ops_after).code, "W1929_ERR_SEND_AFTER_CLOSE")

    def test_05_closure_and_select_close_right_escapes_rejected(self):
        ops_c = [ChanOp("make_chan", "ch", "main"), ChanOp("escape_close_closure", "ch", "main")]
        self.assertEqual(verify_close_confinement(ops_c).code, "W1929_ERR_CLOSURE_CLOSE_ESCAPE")

        ops_s = [ChanOp("make_chan", "ch", "main"), ChanOp("escape_close_select", "ch", "main")]
        self.assertEqual(verify_close_confinement(ops_s).code, "W1929_ERR_SELECT_CLOSE_ESCAPE")


if __name__ == "__main__":
    unittest.main()
