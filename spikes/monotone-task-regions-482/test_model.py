"""Executable boundary and schedule-coverage checks for the abstract machine."""

from dataclasses import replace
from itertools import combinations
import unittest

from model import (Op, Program, answer, closure_oracle, complete, experiment,
                   explore, initial_state, snapshot, step, summarize, validate)


def op(kind, *facts):
    return Op(kind, frozenset(facts))


class MonotoneModelTests(unittest.TestCase):
    def program(self, *tasks, initial=()):
        return Program(frozenset((0, 1, 2)), frozenset(initial), tuple(tasks))

    def test_all_six_interleavings_of_two_two_step_tasks_are_enumerated(self):
        program = self.program((op("put", 0), op("put", 1)),
                               (op("put", 1), op("put", 2)))
        leaves = list(explore(program))
        # Independent combinatorial oracle: choose the two positions of task 0.
        schedules = {tuple(0 if i in positions else 1 for i in range(4))
                     for positions in combinations(range(4), 2)}
        self.assertEqual({leaf.schedule for leaf in leaves}, schedules)
        self.assertEqual(len(leaves), 6)
        self.assertEqual({answer(program, leaf) for leaf in leaves},
                         {((0, 1, 2), ((), ()))})

    def test_put_is_idempotent_returns_unit_and_snapshot_is_sorted(self):
        program = self.program((op("put", 2, 0), op("put", 0)), initial=(1,))
        leaf, = explore(program)
        self.assertEqual(answer(program, leaf), ((0, 1, 2), ((),)))

    def test_await_returns_only_fixed_threshold_even_if_cell_is_larger(self):
        program = self.program((op("await", 0),), initial=(0, 1, 2))
        leaf, = explore(program)
        self.assertEqual(leaf.state.observations, (((0,),),))

    def test_set_threshold_waits_for_all_members(self):
        program = self.program((op("await", 0, 1),), (op("put", 1),), initial=(0,))
        self.assertIsNone(step(program, initial_state(program), 0))
        leaf, = explore(program)
        self.assertEqual(leaf.schedule, (1, 0))
        self.assertEqual(leaf.state.observations, (((0, 1),), ()))

    def test_observations_follow_task_identity_and_local_order(self):
        program = self.program((op("await", 0), op("await", 1)),
                               (op("await", 1),), initial=(0, 1))
        leaves = list(explore(program))
        self.assertEqual(len(leaves), 3)
        self.assertEqual({leaf.state.observations for leaf in leaves},
                         {(((0,), (1,)), ((1,),))})

    def test_unseeded_cycle_is_blocked_and_not_a_join(self):
        program = self.program((op("await", 1), op("put", 0)),
                               (op("await", 0), op("put", 1)))
        leaf, = explore(program)
        self.assertFalse(leaf.complete)
        self.assertEqual(leaf.schedule, ())
        self.assertEqual(leaf.state, closure_oracle(program))
        with self.assertRaisesRegex(ValueError, "joined"):
            snapshot(program, leaf.state)
        with self.assertRaisesRegex(ValueError, "blocked"):
            answer(program, leaf)
        seeded = replace(program, initial=frozenset((0,)))
        self.assertTrue(all(leaf.complete for leaf in explore(seeded)))

    def test_completed_sibling_does_not_allow_snapshot_of_blocked_region(self):
        program = self.program((op("put", 0),), (op("await", 1),))
        leaf, = explore(program)
        self.assertEqual(leaf.state.pcs, (1, 0))
        self.assertFalse(leaf.complete)
        with self.assertRaisesRegex(ValueError, "joined"):
            snapshot(program, leaf.state)

    def test_runnable_region_cannot_be_snapshotted_early(self):
        program = self.program((op("put", 0),))
        with self.assertRaisesRegex(ValueError, "joined"):
            snapshot(program, initial_state(program))

    def test_empty_region_has_one_completed_empty_schedule(self):
        program = self.program(initial=(2,))
        leaf, = explore(program)
        self.assertTrue(complete(program, leaf.state))
        self.assertEqual(answer(program, leaf), ((2,), ()))

    def test_blocked_leaves_are_not_pruned_or_counted_as_completed(self):
        program = self.program((op("put", 0), op("await", 2)),
                               (op("put", 1), op("await", 2)))
        leaves = list(explore(program))
        self.assertEqual(len(leaves), 2)
        self.assertFalse(any(leaf.complete for leaf in leaves))
        self.assertEqual({leaf.state for leaf in leaves}, {closure_oracle(program)})
        self.assertEqual(summarize(program)["blocked_schedules"], 2)

    def test_admission_rejects_forbidden_effects_and_read_interfaces(self):
        for kind in ("remove", "absence", "live_read", "iterate", "first_winner",
                     "timeout", "mutable_ref", "foreign_call", "unknown"):
            with self.subTest(kind=kind), self.assertRaisesRegex(ValueError, "forbidden"):
                validate(self.program((op(kind),)))

    def test_admission_rejects_empty_or_nonfixed_thresholds_and_bad_facts(self):
        invalid = (op("await"), Op("await", {0}), op("put", 3),
                   op("put", True), op("put", "0"), Op("put", frozenset(((0,),))))
        for instruction in invalid:
            with self.subTest(instruction=instruction), self.assertRaises(ValueError):
                validate(self.program((instruction,)))
        with self.assertRaises(ValueError):
            validate(replace(self.program(), initial=frozenset((3,))))
        with self.assertRaises(ValueError):
            validate(replace(self.program(), universe=frozenset((False,))))

    def test_admission_rejects_mutable_task_programs_and_unknown_instructions(self):
        for tasks in ([], ([op("put", 0)],), ((object(),),)):
            with self.subTest(tasks=tasks), self.assertRaises(ValueError):
                validate(replace(self.program(), tasks=tasks))

    def test_each_mutant_has_two_replayable_distinct_completed_answers(self):
        examples = {
            "live_read": self.program((op("put", 0),), (op("live_read"),)),
            "put_flag": self.program((op("put", 0),), (op("put", 0),)),
            "overwrite": self.program((op("put", 0),), (op("put", 1),)),
        }
        for mutant, program in examples.items():
            with self.subTest(mutant=mutant):
                report = summarize(program, mutant=mutant)
                self.assertEqual(report["distinct_completed_answers"], 2)
                self.assertEqual(report["blocked_schedules"], 0)
                answers = set()
                for witness in report["witnesses"]:
                    state = initial_state(program)
                    for task_id in witness["schedule"]:
                        state = step(program, state, task_id, mutant=mutant)
                        self.assertIsNotNone(state)
                    self.assertTrue(complete(program, state))
                    self.assertEqual(snapshot(program, state), witness["snapshot"])
                    self.assertEqual(state.observations, witness["task_observations"])
                    answers.add((snapshot(program, state), state.observations))
                self.assertEqual(len(answers), 2)

    def test_normal_put_controls_have_one_answer(self):
        for right in (0, 1):
            program = self.program((op("put", 0),), (op("put", right),))
            self.assertEqual(summarize(program)["distinct_completed_answers"], 1)
        with self.assertRaisesRegex(ValueError, "mutant"):
            list(explore(self.program(), mutant="typo"))

    def test_preregistered_cases_and_generated_family_match_independent_oracle(self):
        report = experiment()
        self.assertEqual(report["cases"]["diamond"]["witnesses"][0]["snapshot"], (0, 1, 2, 3))
        generated = report["generated"]
        # 6 instructions; 1 + 6 + 6**2 task programs; 4 possible initial sets.
        self.assertEqual(generated["programs"], 4 * 43 * 43)
        self.assertEqual(generated["completed_programs"] + generated["blocked_programs"],
                         generated["programs"])
        self.assertGreater(generated["completed_schedules"], 0)
        self.assertGreater(generated["blocked_schedules"], 0)
        self.assertEqual(generated["oracle_comparisons"],
                         generated["completed_schedules"] + generated["blocked_schedules"])


if __name__ == "__main__":
    unittest.main()
