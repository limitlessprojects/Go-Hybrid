"""Nondeterministic finite policies for volunteer rosters (issue #4896).

A *policy* is a finite-state strategy over a **visible** alphabet (volunteer
names).  Each step consumes one visible symbol and may branch over **private**
choices.  Private choices are invisible to the caller: they exist only to
witness that a published roster is feasible.

Private state
-------------
``(previous, counts, age)``

``previous``  last volunteer consumed, ``""`` before the first shift.
``counts``    per-symbol use counts, aligned with ``Policy.alphabet``.
``age``       ``None``  -> no training handoff designated yet
              ``k >= 0`` -> handoff was designated ``k`` steps ago

``age`` is deliberately a *relative* quantity so that two different visible
prefixes that reach the same (previous, counts, age) share future counts: that
is what keeps the memorised count table small.

The transition function returns a **tuple**, not a set, and duplicates are
preserved on purpose.  A duplicate entry models the "equivalent refactor"
scenario of the issue's M2 milestone: the same private branch written twice.
Determinisation collapses duplicates (it takes a set); path counting does not.
``split_select`` is that switch.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Policy:
    """A bounded, pure step function plus its validity predicate."""

    name: str
    alphabet: str
    horizon: int
    caps: tuple[tuple[str, int], ...]
    handoff: tuple[str, str] = ("A", "B")
    require_handoff: bool = True
    #: Emit the "designate handoff" branch twice while the *last* symbol of the
    #: alphabet (the flex volunteer) has not been used yet.  Purely syntactic:
    #: both copies produce the same private state, so the visible language is
    #: unchanged and only the number of accepting *runs* moves.
    split_select: bool = False

    # ------------------------------------------------------------------ setup
    def cap(self, sym: str) -> int:
        for s, c in self.caps:
            if s == sym:
                return c
        return self.horizon

    @property
    def start(self) -> tuple:
        return ("", (0,) * len(self.alphabet), None)

    def index(self, sym: str) -> int:
        return self.alphabet.index(sym)

    # ------------------------------------------------------------- transition
    def step(self, state: tuple, sym: str) -> tuple[tuple, ...]:
        """Consume one visible symbol; return every feasible private successor."""
        previous, counts, age = state
        if sym == previous:  # nobody serves twice in a row
            return ()
        i = self.index(sym)
        used = counts[i] + 1
        if used > self.cap(sym):
            return ()
        new_counts = counts[:i] + (used,) + counts[i + 1:]
        new_age = None if age is None else age + 1
        out: list[tuple] = [(sym, new_counts, new_age)]
        if age is None and (previous, sym) == self.handoff:
            out.append((sym, new_counts, 0))
            if self.split_select and counts[-1] == 0:
                out.append((sym, new_counts, 0))  # second, equivalent encoding
        return tuple(out)

    def accepting(self, state: tuple) -> bool:
        if not self.require_handoff:
            return True
        return state[2] is not None

    # ------------------------------------------------------------ inspection
    def describe(self) -> str:
        caps = ", ".join(f"{s}<={c}" for s, c in self.caps)
        tail = " + split-select" if self.split_select else ""
        return (f"{self.name}: |Sigma|={len(self.alphabet)} ({self.alphabet}), "
                f"n={self.horizon}, {caps}, handoff {self.handoff[0]}->{self.handoff[1]}{tail}")

    def words(self) -> int:
        """Upper bound on the number of candidate visible words (for scale notes)."""
        return len(self.alphabet) ** self.horizon


# --------------------------------------------------------------------- library
POLICY_A = Policy(
    name="A/8-shifts-issue-4896",
    alphabet="ABC",
    horizon=8,
    caps=(("C", 2),),
)

POLICY_A_SPLIT = Policy(
    name="A/8-shifts-issue-4896+split",
    alphabet="ABC",
    horizon=8,
    caps=(("C", 2),),
    split_select=True,
)

POLICY_B = Policy(
    name="B/12-shifts-4-volunteers",
    alphabet="ABCD",
    horizon=12,
    caps=(("C", 3), ("D", 2)),
)


def scale_policy(horizon: int) -> Policy:
    """Fixed constraint family, only the horizon grows (H4)."""
    return Policy(
        name=f"C/{horizon}-shifts-4-volunteers",
        alphabet="ABCD",
        horizon=horizon,
        caps=(("C", 2), ("D", 1)),
    )
