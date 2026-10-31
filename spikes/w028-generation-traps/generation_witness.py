#!/usr/bin/env python3
"""
W-028 Generation Traps: Runtime Witness & Side-Table Engine.
Implements the reference runtime side table and compiler instrumentation pass
that converts HYB-E3003 compile walls into deterministic runtime witnesses.
"""

import sys
import time
import threading
from typing import Dict, Tuple, Optional, List
from dataclasses import dataclass


class StaleGenerationTrap(Exception):
    """
    Deterministic runtime panic triggered when writing through a stale alias.
    """
    def __init__(self, obj_id: int, field: str, minted_gen: int, current_gen: int, source_loc: str):
        self.obj_id = obj_id
        self.field = field
        self.minted_gen = minted_gen
        self.current_gen = current_gen
        self.source_loc = source_loc
        super().__init__(
            f"stale generation on '{field}' (minted gen {minted_gen}, current gen {current_gen}) — moved at {source_loc}"
        )


class GenerationSideTable:
    """
    Thread-safe 64-bit generation counter side-table for moved composites.
    """
    def __init__(self):
        self._lock = threading.Lock()
        self._table: Dict[int, int] = {}

    def mint(self, obj_id: int) -> int:
        """Mints initial generation 1 for a newly allocated composite."""
        with self._lock:
            self._table[obj_id] = 1
            return 1

    def bump(self, obj_id: int) -> int:
        """Atomically increments generation on move across task/channel boundary."""
        with self._lock:
            current = self._table.get(obj_id, 1)
            new_gen = current + 1
            self._table[obj_id] = new_gen
            return new_gen

    def get_generation(self, obj_id: int) -> int:
        with self._lock:
            return self._table.get(obj_id, 1)

    def witness(self, obj_id: int, minted_token: int, field: str = "val", source_loc: str = "unknown:0"):
        """
        Runtime witness assertion placed before alias mutation.
        Traps deterministically if generation has changed.
        """
        with self._lock:
            current = self._table.get(obj_id, 1)
        if current != minted_token:
            raise StaleGenerationTrap(obj_id, field, minted_token, current, source_loc)


class CompilerWitnessPass:
    """
    Static compiler pass that determines whether to emit compile error (HYB-E3003)
    or instrument the runtime witness trap.
    """
    def __init__(self, deny_stale_alias: bool = False):
        self.deny_stale_alias = deny_stale_alias
        self.side_table = GenerationSideTable()

    def analyze_write(self, obj_id: int, alias_gen: int, field: str, source_loc: str) -> Optional[str]:
        """
        Analyzes an alias mutation.
        In strict mode (--deny-stale-alias), emits compile error HYB-E3003.
        In default mode, emits advisory warning and verifies runtime witness.
        """
        current_gen = self.side_table.get_generation(obj_id)
        if current_gen != alias_gen:
            if self.deny_stale_alias:
                return f"HYB-E3003: use of moved value '{field}' after ownership transfer at {source_loc}"
            else:
                # Advisory warning + runtime witness check
                self.side_table.witness(obj_id, alias_gen, field, source_loc)
        return None


def run_benchmark(iterations: int = 100_000) -> Tuple[float, float, float]:
    """
    Measures latency with baseline writes vs generation-witness-checked writes.
    Returns (baseline_sec, witness_sec, overhead_percent).
    """
    side_table = GenerationSideTable()
    obj_id = 0xDEADBEEF
    token = side_table.mint(obj_id)

    # 1. Baseline raw writes
    t0 = time.perf_counter()
    val = 0
    for i in range(iterations):
        val = i
    baseline_time = time.perf_counter() - t0

    # 2. Witness-checked writes
    t1 = time.perf_counter()
    for i in range(iterations):
        side_table.witness(obj_id, token, "val", "bench.hy:42")
        val = i
    witness_time = time.perf_counter() - t1

    overhead_pct = ((witness_time - baseline_time) / (witness_time if witness_time > 0 else 1)) * 100
    return baseline_time, witness_time, overhead_pct


if __name__ == "__main__":
    print("Running W-028 Generation Witness Benchmark...")
    base, wit, pct = run_benchmark(50_000)
    print(f"Baseline: {base:.5f}s | Witness: {wit:.5f}s | Overhead: {pct:.2f}%")
