"""Unit tests for W-2009 (#2009): Same-Goroutine recover() & Defer Panic-Safety Checker."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from defer_recover_checker import DeferStmt, check_defer_panic_safety  # noqa: E402


class TestW2009DeferRecoverPanicSafety(unittest.TestCase):
    def test_01_direct_recover_in_deferred_literal_succeeds_and_catches_panic(self):
        stmts = [DeferStmt("defer_literal", captures=("guard",), calls_recover_directly=True)]
        res = check_defer_panic_safety(stmts, simulated_panic="boom")
        self.assertTrue(res.ok)
        self.assertEqual(res.code, "OK")
        self.assertEqual(res.recovered_panic_value, "boom")

    def test_02_indirect_helper_recover_and_exit_channel_lowering_rejected(self):
        r1 = check_defer_panic_safety([DeferStmt("defer_literal", calls_helper_with_recover=True)])
        self.assertFalse(r1.ok)
        self.assertEqual(r1.code, "W2009_ERR_INDIRECT_HELPER_RECOVER")

        r2 = check_defer_panic_safety([DeferStmt("plain_recover")])
        self.assertFalse(r2.ok)
        self.assertEqual(r2.code, "W2009_ERR_RECOVER_OUTSIDE_DIRECT_DEFER")

        r3 = check_defer_panic_safety(
            [DeferStmt("defer_literal", calls_recover_directly=True, lower_via_exit_channel=True)],
            simulated_panic="boom",
        )
        self.assertFalse(r3.ok)
        self.assertEqual(r3.code, "W2009_ERR_EXIT_CHANNEL_BREAKS_RECOVER")
        self.assertIsNone(r3.recovered_panic_value)

    def test_03_defer_capturing_already_moved_task_binding_rejected_at_defer_site(self):
        stmts = [
            DeferStmt("task_move", target="buf"),
            DeferStmt("defer_literal", captures=("buf",)),
        ]
        res = check_defer_panic_safety(stmts)
        self.assertFalse(res.ok)
        self.assertEqual(res.code, "W2009_ERR_DEFER_CAPTURES_MOVED_BINDING")

    def test_04_defer_close_followed_by_send_in_same_block_rejected(self):
        stmts = [
            DeferStmt("defer_close", target="ch"),
            DeferStmt("chan_send", target="ch"),
        ]
        res = check_defer_panic_safety(stmts)
        self.assertFalse(res.ok)
        self.assertEqual(res.code, "W2009_ERR_SEND_AFTER_DEFER_CLOSE")

    def test_05_defer_inside_loop_rejected_for_unbounded_accumulation(self):
        res = check_defer_panic_safety([DeferStmt("defer_literal", in_loop=True)])
        self.assertFalse(res.ok)
        self.assertEqual(res.code, "W2009_ERR_UNBOUNDED_LOOP_DEFER")


if __name__ == "__main__":
    unittest.main()
