"""Tests for W-4802: Fork/Join Event Log Oracle and Process Alignment."""

import os
import sys
import unittest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from log_oracle import (
    PrecedenceGraph,
    EventLogOracle,
    topological_oracle_distance,
)


class TestForkJoinLogOracle(unittest.TestCase):
    def test_two_workers_exact_log_has_attribution_ambiguity(self):
        """Two workers both emit 'enter', 'done'. Log 'enter, done, enter, done' matches distance 0
        with attribution ambiguity between worker_1 and worker_2."""
        graph = PrecedenceGraph()
        # Worker 1: 0 (enter) -> 1 (done)
        graph.add_event(0, "enter", "worker_1")
        graph.add_event(1, "done", "worker_1", predecessors=(0,))

        # Worker 2: 2 (enter) -> 3 (done)
        graph.add_event(2, "enter", "worker_2")
        graph.add_event(3, "done", "worker_2", predecessors=(2,))

        oracle = EventLogOracle(graph)
        observed = ["enter", "done", "enter", "done"]
        res = oracle.align(observed)

        self.assertEqual(res.min_distance, 0)
        self.assertGreaterEqual(len(res.legal_event_orders), 2)
        # Either worker_1 or worker_2 could have executed the first 'enter'
        self.assertTrue(res.has_attribution_ambiguity)

    def test_falsifier_inverted_log_needs_two_missed_records(self):
        """Log ['done', 'enter'] cannot execute in that order; needs 2 missed records (cost 2)."""
        graph = PrecedenceGraph()
        graph.add_event(0, "enter", "worker_1")
        graph.add_event(1, "done", "worker_1", predecessors=(0,))

        oracle = EventLogOracle(graph)
        observed = ["done", "enter"]
        res = oracle.align(observed)

        # Distance is 2 (e.g. skip 'done' as extra, match 'enter', miss 'done', etc.)
        self.assertEqual(res.min_distance, 2)

    def test_empty_log_equals_number_of_missed_events(self):
        """An empty log costs exactly N (number of events in DAG, all missed)."""
        graph = PrecedenceGraph()
        graph.add_event(0, "a", "t1")
        graph.add_event(1, "b", "t1", predecessors=(0,))
        graph.add_event(2, "c", "t1", predecessors=(1,))

        oracle = EventLogOracle(graph)
        res = oracle.align([])
        self.assertEqual(res.min_distance, 3)

    def test_independent_topological_oracle_agreement(self):
        """Cross-validation: EventLogOracle distance matches topological oracle across multiple shapes."""
        graph = PrecedenceGraph()
        # Diamond DAG: 0 -> 1, 0 -> 2, (1, 2) -> 3
        graph.add_event(0, "start", "coord")
        graph.add_event(1, "branch_a", "worker_a", predecessors=(0,))
        graph.add_event(2, "branch_b", "worker_b", predecessors=(0,))
        graph.add_event(3, "join", "coord", predecessors=(1, 2))

        oracle = EventLogOracle(graph)

        test_logs = [
            ["start", "branch_a", "branch_b", "join"],
            ["start", "branch_b", "branch_a", "join"],
            ["start", "join"],                      # 2 missed
            ["branch_a", "start", "join"],          # 1 extra, 1 missed
            ["start", "branch_a", "extra", "join"], # 1 extra, 1 missed
            ["noise", "start", "branch_b", "join"], # 1 extra, 1 missed
        ]

        for log in test_logs:
            algo_dist = oracle.align(log).min_distance
            indep_dist = topological_oracle_distance(graph, log)
            self.assertEqual(algo_dist, indep_dist, f"Mismatch for log {log}: algo={algo_dist}, oracle={indep_dist}")


if __name__ == "__main__":
    unittest.main()
