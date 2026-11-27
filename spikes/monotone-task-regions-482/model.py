#!/usr/bin/env python3
"""Finite abstract machine for #482; not a Hybrid checker or Go runtime.

Standard library only. Run this file to reproduce the bounded experiment.
"""

from dataclasses import dataclass, replace
from itertools import combinations, product
import json


@dataclass(frozen=True)
class Op:
    kind: str
    facts: frozenset[int] = frozenset()


@dataclass(frozen=True)
class Program:
    universe: frozenset[int]
    initial: frozenset[int]
    tasks: tuple[tuple[Op, ...], ...]


@dataclass(frozen=True)
class State:
    cell: frozenset[int]
    pcs: tuple[int, ...]
    observations: tuple[tuple, ...]


@dataclass(frozen=True)
class Terminal:
    state: State
    schedule: tuple[int, ...]
    complete: bool


def validate(program, *, allow_live_read=False):
    """Validate only this finite IR. No source-language admission is claimed."""
    def facts(value):
        if type(value) is not frozenset or any(type(x) is not int for x in value):
            raise ValueError("facts must be immutable finite sets of integers")
        if not value <= program.universe:
            raise ValueError("fact outside the closed universe")

    facts(program.universe)
    facts(program.initial)
    if type(program.tasks) is not tuple:
        raise ValueError("tasks must be fixed tuples")
    for task in program.tasks:
        if type(task) is not tuple:
            raise ValueError("task programs must be fixed tuples")
        for op in task:
            if type(op) is not Op:
                raise ValueError("unknown instruction")
            facts(op.facts)
            if op.kind not in ("put", "await"):
                if not (allow_live_read and op.kind == "live_read" and not op.facts):
                    raise ValueError("unknown or forbidden operation: " + op.kind)
            if op.kind == "await" and not op.facts:
                raise ValueError("await needs a fixed nonempty positive threshold")


def initial_state(program):
    return State(program.initial, (0,) * len(program.tasks), ((),) * len(program.tasks))


def complete(program, state):
    return all(pc == len(task) for pc, task in zip(state.pcs, program.tasks))


def snapshot(program, state):
    if not complete(program, state):
        raise ValueError("snapshot requires every region task to have joined")
    return tuple(sorted(state.cell))


def step(program, state, task_id, *, mutant=None):
    """One atomic task step, or None for a completed/blocked task.

    Mutants intentionally violate the proposed interface and are never admitted
    by the normal experiment path. Physical synchronization is not modeled.
    """
    pc = state.pcs[task_id]
    if pc == len(program.tasks[task_id]):
        return None
    op = program.tasks[task_id][pc]
    cell = state.cell
    observation = ()
    if op.kind == "await":
        if not op.facts <= cell:
            return None
        observation = (tuple(sorted(op.facts)),)
    elif op.kind == "put":
        if mutant == "put_flag":
            observation = (not op.facts <= cell,)
        cell = op.facts if mutant == "overwrite" else cell | op.facts
    elif op.kind == "live_read" and mutant == "live_read":
        observation = (tuple(sorted(cell)),)
    else:
        raise ValueError("unmodeled instruction")
    pcs = list(state.pcs)
    pcs[task_id] += 1
    observations = list(state.observations)
    observations[task_id] += observation
    return State(cell, tuple(pcs), tuple(observations))


def explore(program, *, mutant=None):
    """Enumerate all maximal schedules, without pruning blocked leaves.

    Each transition consumes one instruction, so the tree is finite. No idle
    transitions or infinite unfair scheduler traces are represented.
    """
    if mutant not in (None, "live_read", "put_flag", "overwrite"):
        raise ValueError("unknown mutant")
    validate(program, allow_live_read=mutant == "live_read")
    pending = [(initial_state(program), ())]
    while pending:
        state, schedule = pending.pop()
        successors = []
        for task_id in range(len(program.tasks)):
            successor = step(program, state, task_id, mutant=mutant)
            if successor is not None:
                successors.append((successor, schedule + (task_id,)))
        if successors:
            pending.extend(reversed(successors))
        else:
            yield Terminal(state, schedule, complete(program, state))


def closure_oracle(program):
    """Independent round-based interpreter for admitted programs only.

    Does not call step/explore. Advance known positive dependencies until a
    fixed point, including blocked program counters and per-task observations.
    """
    validate(program)
    known = set(program.initial)
    positions = [0] * len(program.tasks)
    observed = [[] for _ in program.tasks]
    changed = True
    while changed:
        changed = False
        for task_id, instructions in enumerate(program.tasks):
            while positions[task_id] < len(instructions):
                op = instructions[positions[task_id]]
                if op.kind == "put":
                    known.update(op.facts)
                elif all(fact in known for fact in op.facts):
                    observed[task_id].append(tuple(sorted(op.facts)))
                else:
                    break
                positions[task_id] += 1
                changed = True
    return State(frozenset(known), tuple(positions), tuple(tuple(x) for x in observed))


def answer(program, terminal):
    if not terminal.complete:
        raise ValueError("blocked execution has no completed answer")
    return snapshot(program, terminal.state), terminal.state.observations


def summarize(program, *, mutant=None):
    completed = blocked = 0
    answers = {}
    for terminal in explore(program, mutant=mutant):
        if terminal.complete:
            completed += 1
            answers.setdefault(answer(program, terminal), terminal.schedule)
        else:
            blocked += 1
    result = {
        "completed_schedules": completed,
        "blocked_schedules": blocked,
        "distinct_completed_answers": len(answers),
    }
    # Preserve two actual schedules, not just a count of divergent outcomes.
    result["witnesses"] = [
        {"schedule": schedule, "snapshot": value[0], "task_observations": value[1]}
        for value, schedule in list(answers.items())[:2]
    ]
    return result


def subsets(universe):
    ordered = sorted(universe)
    for size in range(len(ordered) + 1):
        for items in combinations(ordered, size):
            yield frozenset(items)


def generated_experiment():
    universe = frozenset((0, 1))
    nonempty = list(subsets(universe))[1:]
    instructions = tuple(Op(kind, facts) for kind in ("put", "await") for facts in nonempty)
    tasks = [()]
    for size in (1, 2):
        tasks.extend(product(instructions, repeat=size))
    counts = dict(programs=0, completed_schedules=0, blocked_schedules=0,
                  completed_programs=0, blocked_programs=0, oracle_comparisons=0)
    for initial, left, right in product(subsets(universe), tasks, tasks):
        program = Program(universe, initial, (left, right))
        expected = closure_oracle(program)
        kinds = set()
        for terminal in explore(program):
            counts["oracle_comparisons"] += 1
            if terminal.state != expected:
                raise AssertionError((program, terminal, expected))
            kinds.add(terminal.complete)
            counts["completed_schedules" if terminal.complete else "blocked_schedules"] += 1
        if len(kinds) != 1:
            raise AssertionError("mixed completion and deadlock for an admitted program")
        counts["programs"] += 1
        counts["completed_programs" if True in kinds else "blocked_programs"] += 1
    return counts


def experiment():
    op = lambda kind, *facts: Op(kind, frozenset(facts))
    diamond = Program(frozenset(range(4)), frozenset(), (
        (op("put", 0),),
        (op("await", 0), op("put", 1)),
        (op("await", 0), op("put", 2)),
        (op("await", 1, 2), op("put", 3)),
    ))
    cycle = Program(frozenset((0, 1)), frozenset(), (
        (op("await", 1), op("put", 0)),
        (op("await", 0), op("put", 1)),
    ))
    cases = {
        "diamond": summarize(diamond),
        "unseeded_cycle": summarize(cycle),
        "seeded_cycle": summarize(replace(cycle, initial=frozenset((0,)))),
    }
    mutants = {
        "live_read": Program(frozenset((0,)), frozenset(),
                             ((op("put", 0),), (op("live_read"),))),
        "put_flag": Program(frozenset((0,)), frozenset(),
                            ((op("put", 0),), (op("put", 0),))),
        "overwrite": Program(frozenset((0, 1)), frozenset(),
                             ((op("put", 0),), (op("put", 1),))),
    }
    for name, program in mutants.items():
        cases[name] = summarize(program, mutant=name)
        if cases[name]["distinct_completed_answers"] < 2:
            raise AssertionError("missing negative-control witness: " + name)
    for name in ("diamond", "seeded_cycle"):
        if cases[name]["distinct_completed_answers"] != 1 or cases[name]["blocked_schedules"]:
            raise AssertionError(cases[name])
    if cases["unseeded_cycle"]["completed_schedules"] or not cases["unseeded_cycle"]["blocked_schedules"]:
        raise AssertionError("unseeded cycle incorrectly completed")
    return {"scope": "finite Python IR only; no Hybrid lowering, Go runtime, or timings",
            "cases": cases, "generated": generated_experiment()}


if __name__ == "__main__":
    print(json.dumps(experiment(), indent=2))
