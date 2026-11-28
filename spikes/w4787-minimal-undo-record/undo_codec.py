"""W-4787 (#4787): Provably minimal undo record for lossy task reductions.

Implements combinatorial ranking/unranking for population-count reductions,
emitting the theoretical minimum witness bits ceil(log2(C(n, k))) to enable
exact reverse reconstruction without storing raw input snapshots.
"""

from __future__ import annotations
import math
from dataclasses import dataclass
from typing import List, Tuple


def comb(n: int, k: int) -> int:
    """Computes binomial coefficient C(n, k)."""
    return math.comb(n, k)


def witness_bits_needed(n: int, k: int) -> int:
    """Returns minimum witness bits required: ceil(log2(C(n, k)))."""
    m = comb(n, k)
    if m <= 1:
        return 0
    return math.ceil(math.log2(m))


def rank_flags(flags: List[bool]) -> Tuple[int, int, int]:
    """Computes reduction result (population count k), lexicographic rank, and witness bits needed.

    Iterates through the flags from MSB to LSB.
    Returns: (k, rank, witness_bits).
    """
    n = len(flags)
    k = sum(1 for b in flags if b)
    rem_k = k
    rank = 0

    for i in range(n):
        if rem_k == 0:
            break
        rem_positions = n - 1 - i
        if flags[i]:
            # If current position is 1, all combinations with 0 at this position precede it
            if rem_positions >= rem_k:
                rank += comb(rem_positions, rem_k)
            rem_k -= 1

    return k, rank, witness_bits_needed(n, k)


def unrank_flags(n: int, k: int, rank: int) -> List[bool]:
    """Reconstructs the original n-flag sequence from count k and within-group rank.

    Rejects out-of-range ranks with HYB-E8001.
    """
    total_combinations = comb(n, k)
    if rank < 0 or rank >= total_combinations:
        raise ValueError(
            f"HYB-E8001: Rank out of bounds: rank={rank} not in [0, {total_combinations}) for n={n}, k={k}"
        )

    flags = [False] * n
    rem_k = k
    cur_rank = rank

    for i in range(n):
        if rem_k == 0:
            break
        rem_positions = n - 1 - i
        threshold = comb(rem_positions, rem_k) if rem_positions >= rem_k else 0
        if cur_rank >= threshold:
            flags[i] = True
            cur_rank -= threshold
            rem_k -= 1

    return flags


@dataclass(frozen=True)
class UndoRecord:
    count: int
    rank: int
    bits: int


class LossyReductionEngine:
    """Simulates compile-time lossy reduction with certified minimal undo record."""

    def __init__(self, n: int = 8):
        self.n = n

    def forward_reduce(self, input_flags: List[bool]) -> Tuple[int, UndoRecord]:
        if len(input_flags) != self.n:
            raise ValueError(f"Input flag length must be {self.n}, got {len(input_flags)}")
        k, rank, bits = rank_flags(input_flags)
        return k, UndoRecord(count=k, rank=rank, bits=bits)

    def reverse_reconstruct(self, count: int, record: UndoRecord) -> List[bool]:
        if count != record.count:
            raise ValueError(f"Count mismatch: task output={count}, record={record.count}")
        return unrank_flags(self.n, count, record.rank)
