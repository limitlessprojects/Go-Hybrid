#!/usr/bin/env python3
"""
Test Suite for W-028 Generation Traps: Runtime Witness & Side-Table Engine.
Validates:
1. Deterministic runtime trapping on stale alias writes.
2. Safe read-after-move operations without false panics.
3. Monotonic 64-bit generation updates.
4. 10,000-run deterministic trap integrity.
5. Strict compiler mode parity (--deny-stale-alias -> HYB-E3003).
6. Multi-threaded side-table concurrency.
"""

import sys
import unittest
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from generation_witness import GenerationSideTable, StaleGenerationTrap, CompilerWitnessPass


class TestGenerationTraps(unittest.TestCase):
    def setUp(self):
        self.table = GenerationSideTable()

    def test_stale_alias_write_traps(self):
        """Writing through an alias after an ownership move must raise StaleGenerationTrap."""
        obj_id = 1001
        minted_gen = self.table.mint(obj_id)

        # Move happens: generation bumps to 2
        self.table.bump(obj_id)

        # Attempt to write through stale alias (which still holds minted_gen 1)
        with self.assertRaises(StaleGenerationTrap) as ctx:
            self.table.witness(obj_id, minted_gen, field="worker.status", source_loc="main.hy:15")

        err_msg = str(ctx.exception)
        self.assertIn("stale generation on 'worker.status'", err_msg)
        self.assertIn("moved at main.hy:15", err_msg)
        self.assertIn("minted gen 1, current gen 2", err_msg)

    def test_stale_alias_read_allowed(self):
        """Reads before/after move do not trigger witness assertion."""
        obj_id = 1002
        minted_gen = self.table.mint(obj_id)

        # Before move: witness passes
        self.table.witness(obj_id, minted_gen, field="val", source_loc="main.hy:10")

        # Move happens
        self.table.bump(obj_id)

        # Read-only observation does not call mutation witness; no panic occurs.
        current_gen = self.table.get_generation(obj_id)
        self.assertEqual(current_gen, 2)

    def test_monotonic_generation_bump(self):
        """Generation counter must strictly and monotonically increase."""
        obj_id = 1003
        gen1 = self.table.mint(obj_id)
        self.assertEqual(gen1, 1)

        gen2 = self.table.bump(obj_id)
        self.assertEqual(gen2, 2)

        gen3 = self.table.bump(obj_id)
        self.assertEqual(gen3, 3)

    def test_deterministic_traps_10k_runs(self):
        """Negative control: 10,000 consecutive stale alias writes must trap 10,000 times (0 false accepts)."""
        obj_id = 1004
        minted_gen = self.table.mint(obj_id)
        self.table.bump(obj_id)

        trapped_count = 0
        iterations = 10_000
        for i in range(iterations):
            try:
                self.table.witness(obj_id, minted_gen, field="data", source_loc=f"loop.hy:{i}")
            except StaleGenerationTrap:
                trapped_count += 1

        self.assertEqual(trapped_count, iterations, "Every stale write must trap deterministically without exception")

    def test_deny_stale_alias_flag(self):
        """When --deny-stale-alias is enabled, compiler emits HYB-E3003 instead of runtime trap."""
        compiler_strict = CompilerWitnessPass(deny_stale_alias=True)
        obj_id = 2001
        gen = compiler_strict.side_table.mint(obj_id)
        compiler_strict.side_table.bump(obj_id)

        diag = compiler_strict.analyze_write(obj_id, gen, field="payload", source_loc="server.hy:88")
        self.assertIsNotNone(diag)
        self.assertIn("HYB-E3003", diag)
        self.assertIn("use of moved value 'payload'", diag)

    def test_independent_objects(self):
        """Bumping generation of Object A must never invalidate Object B."""
        objA = 3001
        objB = 3002
        genA = self.table.mint(objA)
        genB = self.table.mint(objB)

        self.table.bump(objA)

        # Object B should remain valid
        self.table.witness(objB, genB, field="b_val", source_loc="test.hy:20")

        # Object A must trap
        with self.assertRaises(StaleGenerationTrap):
            self.table.witness(objA, genA, field="a_val", source_loc="test.hy:22")

    def test_concurrent_witness_thread_safety(self):
        """Concurrent witness queries across multiple threads must remain race-free."""
        obj_id = 4001
        gen = self.table.mint(obj_id)

        def worker():
            for _ in range(500):
                self.table.witness(obj_id, gen, field="shared", source_loc="thread.hy:5")

        threads = [threading.Thread(target=worker) for _ in range(10)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(self.table.get_generation(obj_id), 1)


if __name__ == "__main__":
    unittest.main()
