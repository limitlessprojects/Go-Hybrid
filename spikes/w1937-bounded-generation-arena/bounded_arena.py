"""W-1937 (#1937): Workload-Measured Bounded Borrow/Release Arena with Generation Counters."""

from __future__ import annotations
from dataclasses import dataclass
from typing import Dict, List, Tuple

SIZE_CLASSES = (64, 256, 1024)


@dataclass(frozen=True)
class ArenaHandle:
    shard_id: int
    size_class: int
    slot_idx: int
    generation: int


class BoundedShardArena:
    """Explicit bounded borrow/release arena with per-P shards and generation-counter safety."""

    def __init__(self, num_shards: int = 4, slots_per_class: int = 8) -> None:
        self.num_shards = num_shards
        self.slots_per_class = slots_per_class
        self.generations: Dict[Tuple[int, int, int], int] = {}
        self.in_use: Dict[Tuple[int, int, int], bool] = {}
        self.heap_allocs = 0
        self.arena_reuses = 0
        for s in range(num_shards):
            for c in SIZE_CLASSES:
                for idx in range(slots_per_class):
                    self.generations[(s, c, idx)] = 1
                    self.in_use[(s, c, idx)] = False

    def select_class(self, requested_bytes: int) -> int | None:
        for c in SIZE_CLASSES:
            if requested_bytes <= c:
                return c
        return None

    def borrow(self, shard_id: int, requested_bytes: int) -> Tuple[bool, str, ArenaHandle | None]:
        sc = self.select_class(requested_bytes)
        if sc is None:
            return False, "W1937_ERR_OVERSIZED_ALLOC", None
        s = shard_id % self.num_shards
        for idx in range(self.slots_per_class):
            key = (s, sc, idx)
            if not self.in_use[key]:
                self.in_use[key] = True
                gen = self.generations[key]
                if gen == 1:
                    self.heap_allocs += 1
                else:
                    self.arena_reuses += 1
                return True, "OK", ArenaHandle(s, sc, idx, gen)
        return False, "W1937_ERR_ARENA_CAP_EXCEEDED", None

    def read_slot(self, handle: ArenaHandle) -> Tuple[bool, str]:
        key = (handle.shard_id, handle.size_class, handle.slot_idx)
        if not self.in_use.get(key, False) or self.generations.get(key, -1) != handle.generation:
            return False, "W1937_ERR_STALE_HANDLE_USE_AFTER_RELEASE"
        return True, "OK"

    def release(self, handle: ArenaHandle) -> Tuple[bool, str]:
        key = (handle.shard_id, handle.size_class, handle.slot_idx)
        if self.generations.get(key, -1) != handle.generation or not self.in_use.get(key, False):
            return False, "W1937_ERR_DOUBLE_OR_STALE_RELEASE"
        self.in_use[key] = False
        self.generations[key] += 1
        return True, "OK"

    def gc_reduction_ratio(self) -> float:
        total = self.heap_allocs + self.arena_reuses
        return round(self.arena_reuses / total, 3) if total else 0.0
