"""Determinisation and counting for :mod:`hybrid_roster.policy`.

The visible alphabet is what the caller publishes (the roster string).  The
private choices are existentially quantified: a visible word is feasible iff
*some* private run accepts it.  So the automaton we must count over is the
subset construction:

    start   = { policy.start }
    delta(S, a) = union of policy.step(s, a) for s in S      (a *set*: dedup)
    accepting(S) = any(policy.accepting(s) for s in S)

Because the subset automaton is deterministic, every visible word follows
exactly one path, so ``count(S, r)`` -- the number of distinct accepting
visible suffixes of length ``r`` -- is a plain sum over the alphabet.  This is
the whole trick: counting *words*, not *runs*.  :class:`PathCounter` below does
the naive thing (count runs) and is kept as the falsification control.

All arithmetic uses Python integers, i.e. arbitrary precision; nothing here
ever rounds or floats.
"""

from __future__ import annotations

from .policy import Policy


class SetAutomaton:
    """Subset construction + exact visible-suffix counting.

    ``compact=False`` keeps the raw private state, whose third component is the
    age of the designated handoff (``0`` at the step it was selected, ``k``
    afterwards).  The age is needed to reconstruct *which* boundary was chosen,
    but it never influences what may happen next, so ``compact=True`` collapses
    it to a boolean.  Counting the language must be identical either way; the
    only thing that changes is how many residual sets can coincide.  The scale
    experiment measures that difference, because it is the cost driver.
    """

    def __init__(self, policy: Policy, compact: bool = False) -> None:
        self.policy = policy
        self.compact = compact
        self.start = frozenset({self._canon(policy.start)})
        self._delta: dict[tuple[frozenset, str], frozenset] = {}
        self._count: dict[tuple[frozenset, int], int] = {}
        self.delta_calls = 0
        self.count_calls = 0
        self.peak_members = len(self.start)

    def _canon(self, state: tuple) -> tuple:
        if not self.compact:
            return state
        previous, counts, age = state
        return (previous, counts, None if age is None else True)

    # ------------------------------------------------------------- automaton
    def delta(self, states: frozenset, sym: str) -> frozenset:
        key = (states, sym)
        hit = self._delta.get(key)
        if hit is not None:
            return hit
        out: set[tuple] = set()
        for state in states:
            for succ in self.policy.step(state, sym):
                out.add(self._canon(succ))
        after = frozenset(out)
        self._delta[key] = after
        self.delta_calls += 1
        if len(after) > self.peak_members:
            self.peak_members = len(after)
        return after

    def accepting(self, states: frozenset) -> bool:
        return any(self.policy.accepting(s) for s in states)

    # --------------------------------------------------------------- counting
    def count(self, states: frozenset, remaining: int) -> int:
        """Number of distinct visible words of length ``remaining`` accepted."""
        if not states:
            return 0
        key = (states, remaining)
        hit = self._count.get(key)
        if hit is not None:
            return hit
        self.count_calls += 1
        if remaining == 0:
            value = 1 if self.accepting(states) else 0
        else:
            value = 0
            for sym in self.policy.alphabet:
                value += self.count(self.delta(states, sym), remaining - 1)
        self._count[key] = value
        return value

    @property
    def denominator(self) -> int:
        """Size of the published population: one ticket per visible roster."""
        return self.count(self.start, self.policy.horizon)

    def blocks(self, states: frozenset, left: int) -> list[tuple[str, int]]:
        """``[(symbol, block_size)]`` in alphabet order -- the rank partition.

        ``left`` is the number of symbols still to be placed, *including* the one
        chosen here, so the block size of ``symbol`` is the number of accepting
        continuations of length ``left - 1``.
        """
        return [(sym, self.count(self.delta(states, sym), left - 1))
                for sym in self.policy.alphabet]

    # ------------------------------------------------------------ rank codec
    def decode(self, rank: int) -> str:
        """Inverse of the block partition: rank -> the unique visible word."""
        total = self.denominator
        if not 0 <= rank < total:
            raise ValueError(f"rank {rank} outside [0, {total})")
        states = self.start
        out: list[str] = []
        for left in range(self.policy.horizon, 0, -1):
            for sym, size in self.blocks(states, left):
                if rank < size:
                    out.append(sym)
                    states = self.delta(states, sym)
                    break
                rank -= size
            else:  # pragma: no cover - only reachable if the codec is broken
                raise AssertionError("rank partition did not cover rank")
        return "".join(out)

    def rank_of(self, word: str) -> int:
        """Inverse of :meth:`decode`; raises if ``word`` is not feasible."""
        if len(word) != self.policy.horizon:
            raise ValueError("wrong length")
        states = self.start
        rank = 0
        for pos, sym in enumerate(word):
            remaining = self.policy.horizon - pos
            found = False
            for cand, size in self.blocks(states, remaining):
                if cand == sym:
                    found = True
                    break
                rank += size
            if not found or len(self.delta(states, sym)) == 0:
                raise ValueError(f"{word!r} is not feasible")
            states = self.delta(states, sym)
        if not self.accepting(states):
            raise ValueError(f"{word!r} is not accepting")
        return rank

    # --------------------------------------------------------------- witnesses
    def handoff_positions(self, word: str) -> tuple[int, ...]:
        """Every boundary index that can serve as the training handoff.

        Boundary ``k`` is the step from position ``k`` to ``k + 1``; designating
        it means taking the "select handoff" branch when consuming ``word[k+1]``.
        Feasibility of each candidate is decided by replaying the policy with
        selection forced on exactly that step, so this is a property of the
        *policy*, not of a hard-coded ``A->B`` scan.
        """
        if len(word) != self.policy.horizon:
            raise ValueError("wrong length")
        return tuple(k for k in range(len(word) - 1) if self._forced_ok(word, k))

    def _forced_ok(self, word: str, k: int) -> bool:
        states = frozenset({self.policy.start})
        for pos, sym in enumerate(word):
            selecting_now = pos == k + 1
            nxt: set[tuple] = set()
            for state in states:
                for succ in self.policy.step(state, sym):
                    if (succ[2] == 0) != selecting_now:
                        continue
                    nxt.add(succ)
            if not nxt:
                return False
            states = frozenset(nxt)
        return self.accepting(states)

    def witness(self, word: str) -> int:
        """Canonical, deterministic reveal: the leftmost feasible boundary.

        A function of the *visible* word only, therefore it can never leak a
        private choice.
        """
        positions = self.handoff_positions(word)
        if not positions:
            raise ValueError(f"{word!r} has no feasible handoff")
        return positions[0]

    # ------------------------------------------------------------ diagnostics
    def distinct_sets(self) -> int:
        return len({self.start}
                   | {key[0] for key in self._delta}
                   | set(self._delta.values()))

    def table_sizes(self) -> dict[str, int]:
        return {
            "delta_entries": len(self._delta),
            "count_entries": len(self._count),
            "distinct_sets": len({self.start} | {k[0] for k in self._delta}
                                 | set(self._delta.values())),
            "peak_members": self.peak_members,
        }


class PathCounter:
    """Naive control: count accepting *runs*, not distinct visible words.

    This is the natural thing to write if you think of the policy as a generator
    of witnesses, and it is wrong for the draw: a roster with four handoff
    choices gets four tickets.  Kept in the tree because the contract test
    (H3) needs a concrete implementation that fails it.
    """

    def __init__(self, policy: Policy) -> None:
        self.policy = policy
        self._memo: dict[tuple[tuple, int], int] = {}
        self.calls = 0

    def paths(self, state: tuple, remaining: int) -> int:
        if remaining == 0:
            return 1 if self.policy.accepting(state) else 0
        key = (state, remaining)
        hit = self._memo.get(key)
        if hit is not None:
            return hit
        self.calls += 1
        total = 0
        for sym in self.policy.alphabet:
            for succ in self.policy.step(state, sym):
                total += self.paths(succ, remaining - 1)
        self._memo[key] = total
        return total

    @property
    def denominator(self) -> int:
        return self.paths(self.policy.start, self.policy.horizon)

    def decode(self, rank: int) -> tuple[str, tuple[tuple, ...]]:
        """rank -> (visible word, concrete private run)."""
        total = self.denominator
        if not 0 <= rank < total:
            raise ValueError(f"rank {rank} outside [0, {total})")
        state = self.policy.start
        word: list[str] = []
        run: list[tuple] = []
        for remaining in range(self.policy.horizon - 1, -1, -1):
            placed = False
            for sym in self.policy.alphabet:
                for succ in self.policy.step(state, sym):
                    size = self.paths(succ, remaining)
                    if rank < size:
                        word.append(sym)
                        run.append(succ)
                        state = succ
                        placed = True
                        break
                    rank -= size
                if placed:
                    break
            if not placed:  # pragma: no cover
                raise AssertionError("rank partition did not cover rank")
        return "".join(word), tuple(run)

    def paths_of(self, word: str) -> int:
        """How many accepting runs spell this visible word."""
        count = 0
        frontier: list[tuple[tuple, ...]] = [(self.policy.start,)]
        # small horizons only; used for tables, not for sampling
        for sym in word:
            nxt: list[tuple[tuple, ...]] = []
            for run in frontier:
                for succ in self.policy.step(run[-1], sym):
                    nxt.append(run + (succ,))
            frontier = nxt
        for run in frontier:
            if self.policy.accepting(run[-1]):
                count += 1
        return count
