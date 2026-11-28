#!/usr/bin/env python3
"""recognizer_equivalence.py — Issue #4807 spike.

Compile several revisions of a deterministic, total byte recognizer (a
finite-state automaton: read-only byte loop, finite control state, a
Boolean accept/reject decision at end of input) into the smallest pack of
literal inputs whose answers tell every distinguishable pair of revisions
apart.

Algorithm (both parts are standard automata-theory techniques, as the
issue names):

1. Pairwise/joint equivalence: BFS the product automaton of all revisions
   together. Two revisions are equivalent iff no state reachable in the
   product ever gives them different accept bits — the product state
   space is finite (bounded by the product of per-revision state counts),
   so this is exact, not length-bounded.
2. Minimum test cover: each reachable product state yields an "answer
   vector" (one bit per revision) and a shortest witness string that
   reaches it. Selecting a minimum set of witnesses whose combined
   vectors separate every distinguishable pair of revisions is set cover
   over the universe of distinguishable pairs. Exact set cover is
   exponential; past a budget this falls back to a greedy cover clearly
   marked as an upper bound (the "cost honesty" the issue asks for).
"""

from collections import deque
from itertools import combinations, product

DEAD = -1


class DFA:
    """A deterministic, total recognizer over an explicit transition table.

    Any (state, symbol) missing from `transitions` is an implicit
    transition to the dead/reject sink, keeping the recognizer total
    without writing out every rejecting edge by hand.
    """

    def __init__(self, start, accept, transitions):
        self.start = start
        self.accept = frozenset(accept)
        self.transitions = dict(transitions)

    def step(self, state, symbol):
        if state == DEAD:
            return DEAD
        return self.transitions.get((state, symbol), DEAD)

    def accepts(self, state):
        return state in self.accept

    def run(self, s):
        state = self.start
        for ch in s:
            state = self.step(state, ch)
        return self.accepts(state)


def covered_pairs(vector, n):
    """Pairs of revision indices this answer vector tells apart."""
    return frozenset((i, j) for i, j in combinations(range(n), 2) if vector[i] != vector[j])


def joint_bfs(dfas, alphabet):
    """BFS the product automaton of `dfas` over `alphabet`.

    Returns (vector_witness, reachable):
      vector_witness: dict mapping each distinct answer vector seen to the
        shortest (then lexicographically-least) witness string that
        produces it.
      reachable: list of (joint_state, answer_vector, witness) for every
        reachable product state, in BFS order.
    """
    alphabet = sorted(alphabet)
    start = tuple(d.start for d in dfas)
    witness_of = {start: ""}
    order = [start]
    queue = deque([start])
    while queue:
        joint = queue.popleft()
        prefix = witness_of[joint]
        for sym in alphabet:
            nxt = tuple(d.step(st, sym) for d, st in zip(dfas, joint))
            if nxt not in witness_of:
                witness_of[nxt] = prefix + sym
                order.append(nxt)
                queue.append(nxt)

    reachable = []
    vector_witness = {}
    for joint in order:
        vec = tuple(d.accepts(st) for d, st in zip(dfas, joint))
        reachable.append((joint, vec, witness_of[joint]))
        vector_witness.setdefault(vec, witness_of[joint])
    return vector_witness, reachable


def equivalence_classes(dfas, alphabet):
    """Group revision indices into classes of exact language equivalence.

    Returns (classes, distinguishable): classes is a list of lists of
    indices; distinguishable is the set of index pairs for which some
    reachable product state proves they differ.
    """
    n = len(dfas)
    _, reachable = joint_bfs(dfas, alphabet)
    distinguishable = set()
    for _, vec, _ in reachable:
        distinguishable |= covered_pairs(vec, n)

    parent = list(range(n))

    def find(x):
        while parent[x] != x:
            x = parent[x]
        return x

    for i, j in combinations(range(n), 2):
        if (i, j) not in distinguishable:
            ri, rj = find(i), find(j)
            if ri != rj:
                parent[ri] = rj

    classes = {}
    for i in range(n):
        classes.setdefault(find(i), []).append(i)
    return list(classes.values()), frozenset(distinguishable)


def greedy_cover(candidates, universe):
    """Greedy (non-optimal) set cover, used only once the exact search's
    budget is exhausted. Returns (witnesses, covers_all)."""
    remaining = set(universe)
    chosen = []
    pool = list(candidates)
    while remaining and pool:
        pool.sort(key=lambda c: (-len(c[1] & remaining), len(c[0]), c[0]))
        witness, cov = pool.pop(0)
        gained = cov & remaining
        if not gained:
            break
        chosen.append(witness)
        remaining -= cov
    return sorted(chosen), not remaining


def minimum_test_cover(dfas, alphabet, budget=200_000):
    """Smallest set of witness strings separating every distinguishable
    pair of revisions.

    Exact minimum set cover is tried by brute force over increasing
    subset sizes (fine for the small instances this spike targets: a
    handful of revisions, a handful of distinct answer vectors). If the
    number of combinations tried exceeds `budget` before a cover is
    found, the result falls back to a greedy cover and is marked
    `exact: False, upper_bound: True` rather than silently returned as
    optimal — the cost-honesty case the issue asks for.
    """
    n = len(dfas)
    vector_witness, reachable = joint_bfs(dfas, alphabet)

    universe = set()
    for _, vec, _ in reachable:
        universe |= covered_pairs(vec, n)
    universe = frozenset(universe)

    if not universe:
        return {
            "witnesses": [],
            "exact": True,
            "universe_size": 0,
            "reachable_joint_states": len(reachable),
        }

    candidates = [(w, covered_pairs(vec, n)) for vec, w in vector_witness.items() if covered_pairs(vec, n)]
    candidates.sort(key=lambda c: (len(c[0]), c[0]))

    tried = 0
    for size in range(1, len(candidates) + 1):
        for combo in combinations(candidates, size):
            tried += 1
            if tried > budget:
                witnesses, covers_all = greedy_cover(candidates, universe)
                return {
                    "witnesses": witnesses,
                    "exact": False,
                    "upper_bound": True,
                    "covers_all": covers_all,
                    "universe_size": len(universe),
                    "reachable_joint_states": len(reachable),
                }
            union = frozenset().union(*(c for _, c in combo))
            if union >= universe:
                return {
                    "witnesses": sorted(w for w, _ in combo),
                    "exact": True,
                    "universe_size": len(universe),
                    "reachable_joint_states": len(reachable),
                }
    raise AssertionError("candidate union never covers the universe — joint_bfs/covered_pairs are inconsistent")


def class_vectors_for(dfas, classes, witnesses):
    """The answer vector each equivalence class gives on `witnesses`,
    computed from one representative revision per class."""
    result = {}
    for cid, members in enumerate(classes):
        rep = dfas[members[0]]
        result[cid] = tuple(rep.run(w) for w in witnesses)
    return result


def classify(class_vectors, witnesses, observed):
    """Match a black-box implementation's answers on `witnesses` against
    known class vectors. Returns the matching class id, or None
    ("unknown") if the observed answers match no known class exactly or
    match more than one (an incomplete, non-exact cover can leave two
    classes ambiguous).

    A match only certifies behavior *within the candidate set on this
    pack*: an unmodeled implementation that happens to agree with a known
    class on every witness, but differs somewhere outside the pack, is
    indistinguishable from that class by this check alone.
    """
    observed = tuple(observed)
    matches = [cid for cid, vec in class_vectors.items() if vec == observed]
    return matches[0] if len(matches) == 1 else None


# ---------------------------------------------------------------------------
# Fixture 1: eight number-literal grammars, parameterized by three
# independent flags. Mirrors the issue's own experiment: three probes
# ("+0", "00", "1_0") give a distinct three-bit answer for all eight.
# ---------------------------------------------------------------------------

NUMBER_ALPHABET = tuple("0123456789+_")

_START, _AFTER_PLUS, _AFTER_ZERO, _DIGITS, _AFTER_UNDERSCORE = 0, 1, 2, 3, 4
_DIGIT_CHARS = "0123456789"
_NONZERO_CHARS = "123456789"


def build_number_dfa(allow_plus, allow_leading_zero, allow_underscore):
    """A recognizer for: optional leading '+', then a digit sequence; a
    leading '0' may only be followed by more digits/underscores when
    `allow_leading_zero`; '_' may separate digits when `allow_underscore`
    (but never directly after a lone leading zero, to keep that probe
    isolated to the leading-zero flag alone)."""
    trans = {}

    if allow_plus:
        trans[(_START, "+")] = _AFTER_PLUS
    trans[(_START, "0")] = _AFTER_ZERO
    for d in _NONZERO_CHARS:
        trans[(_START, d)] = _DIGITS

    trans[(_AFTER_PLUS, "0")] = _AFTER_ZERO
    for d in _NONZERO_CHARS:
        trans[(_AFTER_PLUS, d)] = _DIGITS

    if allow_leading_zero:
        for d in _DIGIT_CHARS:
            trans[(_AFTER_ZERO, d)] = _DIGITS

    for d in _DIGIT_CHARS:
        trans[(_DIGITS, d)] = _DIGITS
    if allow_underscore:
        trans[(_DIGITS, "_")] = _AFTER_UNDERSCORE

    for d in _DIGIT_CHARS:
        trans[(_AFTER_UNDERSCORE, d)] = _DIGITS

    return DFA(start=_START, accept={_AFTER_ZERO, _DIGITS}, transitions=trans)


def build_number_grammar_fixture():
    flags = list(product([False, True], repeat=3))
    dfas = [build_number_dfa(*f) for f in flags]
    labels = [
        "plus={} leading_zero={} underscore={}".format(*("Y" if v else "N" for v in f))
        for f in flags
    ]
    return dfas, labels, NUMBER_ALPHABET


# ---------------------------------------------------------------------------
# Fixture 2: four tag-accepting revisions, a reject-everything baseline,
# and a sixth revision that is equivalent to the baseline but reaches it
# through an extra toggling state. Mirrors the issue's claim: the minimum
# cover is four despite only five distinct behaviors, because each tag is
# needed to separate its revision from the baseline class.
# ---------------------------------------------------------------------------

TAG_ALPHABET = tuple("ABCD")


def build_tag_dfa(tag):
    """Accepts exactly the one-character string `tag`; rejects everything
    else (including the empty string and any longer string)."""
    return DFA(start=0, accept={1}, transitions={(0, tag): 1})


def build_reject_all_dfa():
    return DFA(start=0, accept=set(), transitions={})


def build_toggling_reject_dfa():
    """Never accepts, but oscillates between two non-accepting states on
    every symbol — behaviorally equivalent to build_reject_all_dfa()."""
    trans = {}
    for sym in TAG_ALPHABET:
        trans[(0, sym)] = 1
        trans[(1, sym)] = 0
    return DFA(start=0, accept=set(), transitions=trans)


def build_tag_fixture():
    dfas = [
        build_reject_all_dfa(),      # V0: baseline
        build_tag_dfa("A"),           # V1
        build_tag_dfa("B"),           # V2
        build_tag_dfa("C"),           # V3
        build_tag_dfa("D"),           # V4
        build_toggling_reject_dfa(),  # V5: equivalent to V0
    ]
    labels = ["baseline", "tag-A", "tag-B", "tag-C", "tag-D", "toggling-baseline"]
    return dfas, labels, TAG_ALPHABET
