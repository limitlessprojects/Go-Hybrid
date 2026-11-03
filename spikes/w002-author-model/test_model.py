"""Semantic witnesses for the preregistered W-002 fragment."""

import unittest
from unittest.mock import patch

from model import Context, Rejected


class CapabilityTests(unittest.TestCase):
    def setUp(self):
        self.c = Context()
        self.c.allocate("s")

    def test_move_consumes_source(self):
        self.c.move("t", "s")
        self.c.write("t")
        with self.assertRaises(Rejected):
            self.c.get("s")
        with self.assertRaises(Rejected):
            self.c.move("again", "s")

    def test_owner_copy_requires_explicit_intent(self):
        with self.assertRaises(Rejected):
            self.c.copy("alias", "s")
        self.c.write("s")

    def test_fresh_allocations_infer_distinct_ownership(self):
        self.c.allocate("fresh")
        self.assertNotEqual(self.c.get("s").resource, self.c.get("fresh").resource)
        self.c.borrow("r", "s")
        self.c.write("fresh")

    def test_scalar_and_immutable_copy_inference(self):
        self.c.literal("n")
        self.c.literal("text", immutable=True)
        self.c.copy("n2", "n")
        self.c.copy("text2", "text")
        self.assertEqual(self.c.get("n2").kind, "scalar")
        with self.assertRaises(Rejected):
            self.c.write("text2")

    def test_live_borrow_blocks_move(self):
        self.c.borrow("r", "s")
        with self.assertRaises(Rejected):
            self.c.move("t", "s")
        self.c.drop("r")
        self.c.move("t", "s")

    def test_live_borrow_blocks_write_freeze_and_drop(self):
        self.c.borrow("r", "s")
        for operation in (lambda: self.c.write("s"),
                          lambda: self.c.freeze("shared", "s"),
                          lambda: self.c.drop("s")):
            with self.assertRaises(Rejected):
                operation()
        self.c.drop("r")
        self.c.write("s")

    def test_reborrow_keeps_owner_frozen(self):
        self.c.borrow("r", "s")
        self.c.borrow("r2", "r")
        self.c.drop("r")
        with self.assertRaises(Rejected):
            self.c.write("s")
        self.c.drop("r2")
        self.c.write("s")

    def test_moved_capture_consumes_at_construction(self):
        self.c.closure("f", [("move", "s")])
        with self.assertRaises(Rejected):
            self.c.get("s")
        self.c.spawn("f")
        with self.assertRaises(Rejected):
            self.c.spawn("f")

    def test_borrowed_closure_is_local_only(self):
        self.c.closure("f", [("borrow", "s")])
        with self.assertRaises(Rejected):
            self.c.spawn("f")
        with self.assertRaises(Rejected):
            self.c.write("s")
        self.c.call("f")
        self.c.write("s")

    def test_dropped_borrowed_closure_releases_borrow(self):
        self.c.closure("f", [("borrow", "s")])
        self.c.drop("f")
        self.c.write("s")

    def test_moving_a_borrow_into_closure_does_not_launder_it(self):
        self.c.borrow("r", "s")
        self.c.closure("f", [("move", "r")])
        with self.assertRaises(Rejected):
            self.c.spawn("f")
        with self.assertRaises(Rejected):
            self.c.move("t", "s")
        self.c.drop("f")
        self.c.write("s")

    def test_nested_closure_does_not_hide_borrow(self):
        self.c.closure("inner", [("borrow", "s")])
        self.c.closure("outer", [("move", "inner")])
        with self.assertRaises(Rejected):
            self.c.spawn("outer")
        self.c.drop("outer")
        self.c.write("s")

    def test_immutable_capture_can_spawn(self):
        self.c.freeze("shared", "s")
        self.c.closure("f", [("copy", "shared")])
        self.c.spawn("f")
        self.c.copy("shared2", "shared")

    def test_duplicate_capture_rejects_atomically(self):
        before = dict(self.c.bindings)
        with self.assertRaises(Rejected):
            self.c.closure("f", [("move", "s"), ("move", "s")])
        self.assertEqual(self.c.bindings, before)

    def test_invalid_later_capture_does_not_consume_earlier_owner(self):
        before = dict(self.c.bindings)
        with self.assertRaises(Rejected):
            self.c.closure("f", [("move", "s"), ("move", "unknown")])
        self.assertEqual(self.c.bindings, before)

    def test_capture_borrowed_owner_cannot_move(self):
        self.c.borrow("r", "s")
        with self.assertRaises(Rejected):
            self.c.closure("f", [("move", "s")])
        self.c.get("s")

    def test_destination_collision_does_not_lose_owner(self):
        self.c.literal("taken")
        before = dict(self.c.bindings)
        with self.assertRaises(Rejected):
            self.c.move("taken", "s")
        self.assertEqual(self.c.bindings, before)

    def test_unknown_calls_and_unbound_borrows_fail_closed(self):
        with self.assertRaises(Rejected):
            self.c.call("unknown")
        with self.assertRaises(Rejected):
            self.c.borrow("r", "unknown")

    def test_calls_are_affine(self):
        self.c.closure("f", [("move", "s")])
        self.c.call("f")
        with self.assertRaises(Rejected):
            self.c.call("f")

    def test_mutation_witness_for_removed_borrow_guard(self):
        self.c.borrow("r", "s")
        with self.assertRaises(Rejected):
            self.c.move("t", "s")
        # The same witness is wrongly admitted when transfer validation dies.
        with patch.object(Context, "transferable", lambda self, value: None):
            self.c.move("t", "s")
        self.c.get("r")
        self.c.get("t")


if __name__ == "__main__":
    unittest.main()
