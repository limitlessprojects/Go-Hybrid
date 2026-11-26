"""W-4781 (#4781): Certify peak live buffer bytes across schedules via closure analysis.

Computes the exact maximum live buffer bytes over all legal interleavings
in a precedence DAG and returns a witness schedule attaining that peak.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from itertools import permutations
from typing import Dict, List, Optional, Set, Tuple


@dataclass(frozen=True)
class BufferEvent:
    name: str          # e.g. "A+", "A-", "B+", "B-"
    buffer_id: str     # e.g. "A", "B", "C"
    weight_bytes: int  # Positive for acquisition, negative for release
    predecessors: Tuple[str, ...] = ()


@dataclass
class PrecedenceDAG:
    events: Dict[str, BufferEvent] = field(default_factory=dict)

    def add_event(self, name: str, buffer_id: str, weight: int, predecessors: Tuple[str, ...] = ()) -> None:
        self.events[name] = BufferEvent(name, buffer_id, weight, predecessors)


@dataclass(frozen=True)
class PeakCertificationResult:
    certified_peak_bytes: int
    witness_schedule_prefix: List[str]
    full_witness_schedule: List[str]
    total_legal_orders: int
    peak_distribution: Dict[int, int]


class PeakLiveBufferCertifier:
    """Certifies the peak live buffer footprint across all valid execution schedules."""

    def __init__(self, dag: PrecedenceDAG):
        self.dag = dag
        self.all_event_names = list(self.dag.events.keys())

    def enumerate_all_legal_schedules(self) -> List[List[str]]:
        """Enumerates all topologically valid execution sequences."""
        valid_orders: List[List[str]] = []
        n = len(self.all_event_names)

        def backtrack(current_order: List[str], executed_set: Set[str]):
            if len(current_order) == n:
                valid_orders.append(list(current_order))
                return

            for name in self.all_event_names:
                if name not in executed_set:
                    ev = self.dag.events[name]
                    # Check predecessors
                    if all(p in executed_set for p in ev.predecessors):
                        executed_set.add(name)
                        current_order.append(name)
                        backtrack(current_order, executed_set)
                        current_order.pop()
                        executed_set.remove(name)

        backtrack([], set())
        return valid_orders

    def certify_peak(self) -> PeakCertificationResult:
        all_schedules = self.enumerate_all_legal_schedules()
        if not all_schedules:
            raise ValueError("Precedence DAG has no legal execution schedules (contains cycles)")

        peak_distribution: Dict[int, int] = {}
        max_peak = -1
        best_schedule: List[str] = []
        best_prefix: List[str] = []

        for sched in all_schedules:
            live = 0
            sched_peak = 0
            peak_prefix_for_sched: List[str] = []

            cur_prefix = []
            for ev_name in sched:
                ev = self.dag.events[ev_name]
                live += ev.weight_bytes
                cur_prefix.append(ev_name)
                if live > sched_peak:
                    sched_peak = live
                    peak_prefix_for_sched = list(cur_prefix)

            peak_distribution[sched_peak] = peak_distribution.get(sched_peak, 0) + 1

            if sched_peak > max_peak:
                max_peak = sched_peak
                best_schedule = list(sched)
                best_prefix = peak_prefix_for_sched

        return PeakCertificationResult(
            certified_peak_bytes=max_peak,
            witness_schedule_prefix=best_prefix,
            full_witness_schedule=best_schedule,
            total_legal_orders=len(all_schedules),
            peak_distribution=peak_distribution,
        )
