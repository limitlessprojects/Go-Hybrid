"""Unit tests for W-1937 (#1937): Workload-Measured Bounded Borrow/Release Arena."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from bounded_arena import BoundedShardArena  # noqa: E402


class TestW1937BoundedGenerationArena(unittest.TestCase):
    def test_01_histogram_size_classes_and_gc_reduction_above_30_percent(self):
        arena = BoundedShardArena(num_shards=4, slots_per_class=2)
        for _ in range(10):
            ok, code, h = arena.borrow(shard_id=0, requested_bytes=120)
            self.assertTrue(ok)
            self.assertEqual(h.size_class, 256)
            self.assertTrue(arena.release(h)[0])
        self.assertGreaterEqual(arena.gc_reduction_ratio(), 0.30)
        self.assertEqual(arena.gc_reduction_ratio(), 0.90)

    def test_02_double_release_caught_by_generation_counter(self):
        arena = BoundedShardArena(num_shards=2, slots_per_class=2)
        _, _, h = arena.borrow(0, 48)
        self.assertEqual(arena.release(h), (True, "OK"))
        self.assertEqual(arena.release(h), (False, "W1937_ERR_DOUBLE_OR_STALE_RELEASE"))

    def test_03_stale_handle_use_after_reborrow_rejected(self):
        arena = BoundedShardArena(num_shards=1, slots_per_class=1)
        _, _, h1 = arena.borrow(0, 64)
        arena.release(h1)
        _, _, h2 = arena.borrow(0, 64)
        self.assertEqual(h2.generation, 2)
        self.assertEqual(arena.read_slot(h1), (False, "W1937_ERR_STALE_HANDLE_USE_AFTER_RELEASE"))
        self.assertEqual(arena.read_slot(h2), (True, "OK"))

    def test_04_fail_closed_arena_cap_and_oversized_request_rejected(self):
        arena = BoundedShardArena(num_shards=1, slots_per_class=1)
        self.assertTrue(arena.borrow(0, 64)[0])
        self.assertEqual(arena.borrow(0, 64)[1], "W1937_ERR_ARENA_CAP_EXCEEDED")
        self.assertEqual(arena.borrow(0, 4096)[1], "W1937_ERR_OVERSIZED_ALLOC")

    def test_05_concurrent_shards_isolate_fast_paths(self):
        arena = BoundedShardArena(num_shards=4, slots_per_class=1)
        handles = [arena.borrow(shard_id=s, requested_bytes=200)[2] for s in range(4)]
        self.assertEqual(len({h.shard_id for h in handles}), 4)


if __name__ == "__main__":
    unittest.main()
