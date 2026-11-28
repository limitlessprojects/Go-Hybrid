"""Samplers for the three implementations compared in this spike.

Every sampler exposes ``draw(rng) -> (roster, handoff_boundary)`` and a
``denominator`` describing the size of the space it draws from.  All of them are
seeded-only (``random.Random``), so runs are reproducible; unbiasedness of the
integer draw rests on CPython's ``randrange`` rejection sampling, which is exact
for arbitrary integer bounds.

    PairsUniform        uniform over accepted (roster, run) pairs  [wrong #1]
    RosterThenHandoff   uniform roster, then a uniform legal reveal  [wrong #2]
    Determinized        determinised count + rank block             [proposal]
    PathUniform         uniform over accepting runs                  [wrong #1b]

``PairsUniform`` and ``PathUniform`` must be distributionally identical; they
are implemented differently on purpose so that agreement is evidence, not
tautology.
"""

from __future__ import annotations

import random

from .determinize import PathCounter, SetAutomaton
from .oracle import enumerate_pairs
from .policy import Policy


class PairsUniform:
    """Wrong implementation 1: one ticket per (roster, handoff) pair."""

    kind = "pairs-uniform"
    label = "uniform over (roster, handoff) pairs"

    def __init__(self, policy: Policy) -> None:
        pairs = enumerate_pairs(
            policy.alphabet, policy.horizon, policy.caps,
            policy.handoff, policy.require_handoff,
        )
        self._pairs: list[tuple[str, int]] = [
            (word, k) for word, boundaries in pairs.items() for k in boundaries
        ]
        self.denominator = len(self._pairs)
        self._laws: dict[str, list[int]] = {
            word: list(boundaries) for word, boundaries in pairs.items()
        }

    def draw(self, rng: random.Random) -> tuple[str, int]:
        return self._pairs[rng.randrange(self.denominator)]

    def roster_prob(self, word: str) -> float:
        """P(this roster) under this sampler's own law."""
        return len(self._laws[word]) / self.denominator

    def reveal_distribution(self, word: str) -> dict[int, float]:
        return _law_from_boundaries(self._laws[word])


class PathUniform:
    """Wrong implementation 1b: rank-decode the *run* space.

    Same distribution as :class:`PairsUniform` but reached through the policy,
    so it is the version that a compiler could plausibly emit after forgetting
    to determinise.
    """

    kind = "pairs-uniform(path-count)"
    label = "uniform over accepting runs"

    def __init__(self, policy: Policy) -> None:
        self._counter = PathCounter(policy)
        self._policy = policy
        self.denominator = self._counter.denominator

    def draw(self, rng: random.Random) -> tuple[str, int]:
        rank = rng.randrange(self.denominator)
        word, run = self._counter.decode(rank)
        return word, _selected_boundary(run)

    def roster_prob(self, word: str) -> float:
        return self._counter.paths_of(word) / self.denominator

    def reveal_distribution(self, word: str) -> dict[int, float]:
        boundaries = [_selected_boundary(run) for run in _runs_of(self._policy, word)]
        return _law_from_boundaries(boundaries)


class RosterThenHandoff:
    """Wrong implementation 2: uniform roster, then a uniform legal reveal.

    ``reveal`` selects the conditional distribution:

    ``"boundary"``  uniform over the feasible *boundary indices*
    ``"run"``       uniform over the feasible accepting *runs* (encodings)

    The two coincide only when the policy has no duplicate encodings, which is
    exactly the ambiguity the contract test exploits.
    """

    kind = "roster-then-handoff"
    label = "uniform roster, then uniform legal handoff"

    def __init__(self, policy: Policy, reveal: str = "boundary") -> None:
        if reveal not in {"boundary", "run"}:
            raise ValueError("reveal must be 'boundary' or 'run'")
        self._policy = policy
        self._reveal = reveal
        self._automaton = SetAutomaton(policy)
        self.denominator = self._automaton.denominator
        self._choices: dict[str, list[int]] = {}
        if reveal == "run":
            self._build_run_table()

    # ------------------------------------------------------------------ table
    def _build_run_table(self) -> None:
        """For each feasible roster, the boundary chosen by each accepting run."""
        for rank in range(self.denominator):
            word = self._automaton.decode(rank)
            if word not in self._choices:
                self._choices[word] = [_selected_boundary(run)
                                       for run in _runs_of(self._policy, word)]

    def _choice_list(self, word: str) -> list[int]:
        cached = self._choices.get(word)
        if cached is None:
            if self._reveal == "boundary":
                cached = list(self._automaton.handoff_positions(word))
            else:
                cached = [_selected_boundary(run) for run in _runs_of(self._policy, word)]
            self._choices[word] = cached
        return cached

    # ------------------------------------------------------------------- draw
    def draw(self, rng: random.Random) -> tuple[str, int]:
        word = self._automaton.decode(rng.randrange(self.denominator))
        return word, self.sample_reveal(word, rng)

    def sample_reveal(self, word: str, rng: random.Random) -> int:
        """The conditional reveal for an already published roster."""
        choices = self._choice_list(word)
        return choices[rng.randrange(len(choices))]

    def roster_prob(self, word: str) -> float:
        return 1.0 / self.denominator

    def reveal_distribution(self, word: str) -> dict[int, float]:
        """Exact conditional law of the reveal given a published roster."""
        return _law_from_boundaries(self._choice_list(word))


class Determinized:
    """The proposal: exactly one ticket per publishable roster.

    Draw ``rank`` uniformly in ``[0, N)``, then walk the rank partition.  The
    reveal is the canonical witness (leftmost feasible boundary), a function of
    the published word alone.
    """

    kind = "determinized"
    label = "determinised count + rank block"

    def __init__(self, policy: Policy) -> None:
        self._automaton = SetAutomaton(policy)
        self.denominator = self._automaton.denominator
        self._witness: dict[str, int] = {}

    def draw(self, rng: random.Random) -> tuple[str, int]:
        word = self._automaton.decode(rng.randrange(self.denominator))
        return word, self.sample_reveal(word, rng)

    def sample_reveal(self, word: str, rng: random.Random) -> int:
        hit = self._witness.get(word)
        if hit is None:
            hit = self._automaton.witness(word)
            self._witness[word] = hit
        return hit

    def roster_prob(self, word: str) -> float:
        return 1.0 / self.denominator

    def reveal_distribution(self, word: str) -> dict[int, float]:
        return {self._automaton.witness(word): 1.0}


def _selected_boundary(run: tuple[tuple, ...]) -> int:
    """Boundary index designated by a concrete private run.

    ``age == 0`` marks the step at which the handoff branch was taken; that step
    consumes position ``pos``, i.e. boundary ``pos - 1``.
    """
    for pos, state in enumerate(run):
        if state[2] == 0:
            return pos - 1
    raise ValueError("run never designated a handoff")


def _runs_of(policy: Policy, word: str) -> list[tuple[tuple, ...]]:
    """All accepting private runs that spell ``word`` (small horizons only).

    A run is the tuple of private states *after* each visible symbol, so
    ``run[0]`` is the state after ``word[0]``.  This matches
    :meth:`PathCounter.decode`, and makes ``run[k]`` the state after
    ``word[k]`` -- i.e. the state in which boundary ``k - 1`` was designated.
    """
    frontier: list[tuple[tuple, tuple]] = [(policy.start, ())]
    for sym in word:
        nxt: list[tuple[tuple, tuple]] = []
        for last, trail in frontier:
            for succ in policy.step(last, sym):
                nxt.append((succ, trail + (succ,)))
        frontier = nxt
    return [trail for last, trail in frontier if policy.accepting(last)]


def _law_from_boundaries(boundaries: list[int]) -> dict[int, float]:
    law: dict[int, float] = {}
    for k in boundaries:
        law[k] = law.get(k, 0.0) + 1.0 / len(boundaries)
    return law
