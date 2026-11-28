"""Independent whole-roster enumerator -- the oracle.

Deliberately *not* built on the policy automaton.  It enumerates concrete
visible words (depth-first, in lexicographic order) and decides validity with a
predicate written directly on the string:

    no two equal neighbours
    count(sym) <= cap(sym) for every capped symbol
    at least one transition handoff_from -> handoff_to exists (if required)

and then reads the private choices straight off the word:

    handoff candidates = [i for i in range(n-1) if w[i] == from and w[i+1] == to]

That is a different algorithm from subset construction + big-integer suffix
counting, which is what makes the agreement between the two meaningful.

:func:`brute_product_pairs` is a third, even dumber implementation used only
for small horizons to cross-check the DFS itself.
"""

from __future__ import annotations

from itertools import product
from typing import Iterable


def _leaf_boundaries(word: str, handoff: tuple[str, str]) -> tuple[int, ...]:
    a, b = handoff
    return tuple(i for i in range(len(word) - 1)
                 if word[i] == a and word[i + 1] == b)


def enumerate_pairs(
    alphabet: str,
    horizon: int,
    caps: Iterable[tuple[str, int]] = (),
    handoff: tuple[str, str] = ("A", "B"),
    require_handoff: bool = True,
) -> dict[str, tuple[int, ...]]:
    """Return ``{visible word: (handoff boundary, ...)}`` in lexicographic order."""
    cap = {s: c for s, c in caps}
    out: dict[str, tuple[int, ...]] = {}
    letters = tuple(alphabet)

    def walk(prefix: list[str], counts: dict[str, int]) -> None:
        if len(prefix) == horizon:
            word = "".join(prefix)
            boundaries = _leaf_boundaries(word, handoff)
            if require_handoff and not boundaries:
                return
            out[word] = boundaries
            return
        previous = prefix[-1] if prefix else None
        for sym in letters:
            if sym == previous:
                continue
            used = counts.get(sym, 0) + 1
            if sym in cap and used > cap[sym]:
                continue
            prefix.append(sym)
            counts[sym] = used
            walk(prefix, counts)
            counts[sym] = used - 1
            prefix.pop()

    walk([], {})
    return out


def brute_product_pairs(
    alphabet: str,
    horizon: int,
    caps: Iterable[tuple[str, int]] = (),
    handoff: tuple[str, str] = ("A", "B"),
    require_handoff: bool = True,
) -> dict[str, tuple[int, ...]]:
    """Third implementation: materialise every word, then filter.  Small n only."""
    cap = {s: c for s, c in caps}
    out: dict[str, tuple[int, ...]] = {}
    for letters in product(alphabet, repeat=horizon):
        word = "".join(letters)
        if any(x == y for x, y in zip(word, word[1:])):
            continue
        if any(word.count(s) > c for s, c in cap.items()):
            continue
        boundaries = _leaf_boundaries(word, handoff)
        if require_handoff and not boundaries:
            continue
        out[word] = boundaries
    return out


def multiplicity_histogram(pairs: dict[str, tuple[int, ...]]) -> dict[int, int]:
    """How many rosters have exactly k handoff choices."""
    hist: dict[int, int] = {}
    for boundaries in pairs.values():
        hist[len(boundaries)] = hist.get(len(boundaries), 0) + 1
    return dict(sorted(hist.items()))


def totals(pairs: dict[str, tuple[int, ...]]) -> tuple[int, int]:
    """``(number of visible rosters, number of (roster, handoff) pairs)``."""
    return len(pairs), sum(len(b) for b in pairs.values())
