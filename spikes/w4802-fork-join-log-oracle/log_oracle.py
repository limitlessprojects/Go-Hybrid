"""W-4802 (#4802): Compile fork/join blocks into an oracle for incomplete event logs.

Implements optimal alignment over event precedence graphs via memoized search
(completed_events, log_offset) and reports event order, minimum edit distance,
and worker attribution ambiguity.
"""

from __future__ import annotations
from dataclasses import dataclass, field
from itertools import permutations
from typing import Dict, List, Optional, Set, Tuple


@dataclass(frozen=True)
class EventNode:
    event_id: int
    label: str
    task_id: str
    predecessors: Tuple[int, ...] = ()


@dataclass
class PrecedenceGraph:
    nodes: Dict[int, EventNode] = field(default_factory=dict)

    def add_event(self, event_id: int, label: str, task_id: str, predecessors: Tuple[int, ...] = ()) -> None:
        self.nodes[event_id] = EventNode(event_id, label, task_id, predecessors)


@dataclass
class AlignmentResult:
    min_distance: int
    legal_event_orders: List[List[int]]
    edits: List[str]
    has_attribution_ambiguity: bool


class EventLogOracle:
    """Computes minimum alignment distance between an observed event log and legal DAG executions."""

    def __init__(self, graph: PrecedenceGraph):
        self.graph = graph
        self.all_event_ids = tuple(sorted(self.graph.nodes.keys()))
        self.num_events = len(self.all_event_ids)

    def is_enabled(self, event_id: int, completed_mask: int) -> bool:
        node = self.graph.nodes[event_id]
        for pred in node.predecessors:
            if not (completed_mask & (1 << pred)):
                return False
        return True

    def align(self, observed_log: List[str]) -> AlignmentResult:
        """Finds minimum edit distance (missed or extra records) to explain observed_log."""
        memo: Dict[Tuple[int, int], int] = {}
        target_mask = (1 << self.num_events) - 1

        def dp(completed_mask: int, log_idx: int) -> int:
            state = (completed_mask, log_idx)
            if state in memo:
                return memo[state]

            # Base case: all events executed
            if completed_mask == target_mask:
                # Remaining observed records must be treated as extra (cost 1 each)
                res = len(observed_log) - log_idx
                memo[state] = res
                return res

            best_cost = float("inf")

            # Option 1: Skip an observed record as extra/spurious (cost 1)
            if log_idx < len(observed_log):
                cost_skip_log = 1 + dp(completed_mask, log_idx + 1)
                if cost_skip_log < best_cost:
                    best_cost = cost_skip_log

            # Option 2: Fire an enabled event
            for ev_id in self.all_event_ids:
                if not (completed_mask & (1 << ev_id)) and self.is_enabled(ev_id, completed_mask):
                    node = self.graph.nodes[ev_id]
                    next_mask = completed_mask | (1 << ev_id)

                    # Subcase 2a: Matches observed record (cost 0)
                    if log_idx < len(observed_log) and observed_log[log_idx] == node.label:
                        cost_match = dp(next_mask, log_idx + 1)
                        if cost_match < best_cost:
                            best_cost = cost_match

                    # Subcase 2b: Event occurred but was missed from the log (cost 1)
                    cost_missed_event = 1 + dp(next_mask, log_idx)
                    if cost_missed_event < best_cost:
                        best_cost = cost_missed_event

            memo[state] = int(best_cost)
            return int(best_cost)

        min_dist = dp(0, 0)

        # Recover optimal paths and check attribution ambiguity
        optimal_orders: List[List[int]] = []
        sample_edits: List[str] = []

        def recover(completed_mask: int, log_idx: int, current_path: List[int], current_edits: List[str]):
            if completed_mask == target_mask:
                # Extra logs
                tail_edits = list(current_edits)
                for k in range(log_idx, len(observed_log)):
                    tail_edits.append(f"extra_record('{observed_log[k]}')")
                optimal_orders.append(list(current_path))
                if not sample_edits:
                    sample_edits.extend(tail_edits)
                return

            expected_rem = memo.get((completed_mask, log_idx), float("inf"))

            # Check skip log
            if log_idx < len(observed_log):
                if 1 + memo.get((completed_mask, log_idx + 1), float("inf")) == expected_rem:
                    recover(
                        completed_mask,
                        log_idx + 1,
                        current_path,
                        current_edits + [f"extra_record('{observed_log[log_idx]}')"],
                    )

            # Check firing enabled events
            for ev_id in self.all_event_ids:
                if not (completed_mask & (1 << ev_id)) and self.is_enabled(ev_id, completed_mask):
                    node = self.graph.nodes[ev_id]
                    next_mask = completed_mask | (1 << ev_id)

                    if log_idx < len(observed_log) and observed_log[log_idx] == node.label:
                        if memo.get((next_mask, log_idx + 1), float("inf")) == expected_rem:
                            recover(
                                next_mask,
                                log_idx + 1,
                                current_path + [ev_id],
                                current_edits + [f"match_event({ev_id}, '{node.label}', task='{node.task_id}')"],
                            )

                    if 1 + memo.get((next_mask, log_idx), float("inf")) == expected_rem:
                        recover(
                            next_mask,
                            log_idx,
                            current_path + [ev_id],
                            current_edits + [f"missed_event({ev_id}, '{node.label}', task='{node.task_id}')"],
                        )

        recover(0, 0, [], [])

        # Deduplicate optimal orders
        unique_orders: List[List[int]] = []
        seen = set()
        for o in optimal_orders:
            t = tuple(o)
            if t not in seen:
                seen.add(t)
                unique_orders.append(o)

        # Detect task attribution ambiguity:
        # Check if multiple optimal orders attribute a record at the same position to different tasks
        has_ambiguity = False
        if len(unique_orders) > 1:
            for pos in range(min(len(observed_log), self.num_events)):
                attributed_tasks = set()
                for order in unique_orders:
                    if pos < len(order):
                        ev = self.graph.nodes[order[pos]]
                        if ev.label == observed_log[pos]:
                            attributed_tasks.add(ev.task_id)
                if len(attributed_tasks) > 1:
                    has_ambiguity = True
                    break

        return AlignmentResult(
            min_distance=min_dist,
            legal_event_orders=unique_orders,
            edits=sample_edits,
            has_attribution_ambiguity=has_ambiguity,
        )


def topological_oracle_distance(graph: PrecedenceGraph, log: List[str]) -> int:
    """Independent oracle: enumerates all topological orders and computes minimal Levenshtein-like distance."""
    all_events = sorted(graph.nodes.keys())
    valid_topological_orders = []

    for perm in permutations(all_events):
        valid = True
        seen = set()
        for ev_id in perm:
            for pred in graph.nodes[ev_id].predecessors:
                if pred not in seen:
                    valid = False
                    break
            if not valid:
                break
            seen.add(ev_id)
        if valid:
            valid_topological_orders.append(perm)

    best = float("inf")
    for order in valid_topological_orders:
        labels = [graph.nodes[e].label for e in order]
        # Standard LCS / edit distance between labels and log
        n, m = len(labels), len(log)
        dp_table = [[0] * (m + 1) for _ in range(n + 1)]
        for i in range(n + 1):
            dp_table[i][0] = i
        for j in range(m + 1):
            dp_table[0][j] = j
        for i in range(1, n + 1):
            for j in range(1, m + 1):
                if labels[i - 1] == log[j - 1]:
                    dp_table[i][j] = dp_table[i - 1][j - 1]
                else:
                    dp_table[i][j] = 1 + min(dp_table[i - 1][j], dp_table[i][j - 1])
        if dp_table[n][m] < best:
            best = dp_table[n][m]

    return int(best)
