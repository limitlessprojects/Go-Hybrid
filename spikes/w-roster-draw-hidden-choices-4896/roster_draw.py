#!/usr/bin/env python3
"""Enumerate visible rosters fairly despite multiple valid training-handoff choices.

Spike for issue #4896: a coordinator publishes an 8-shift roster over
volunteers A/B/C (nobody serves twice in a row, C takes at most two shifts)
and separately designates one A-to-B handoff as the training slot. Drawing
uniformly over *roster-and-handoff* pairs is unfair to rosters with few
handoff choices (ABABABAB has four choices; ABACACBA has one). This module
determinizes the policy's hidden state, counts accepting suffixes from each
reachable state set with arbitrary-precision integers, and rank-decodes a
draw so every *roster* gets exactly one ticket regardless of how many
private handoff choices back it.

Run with Python 3.10+ using only the standard library.
"""
from collections import Counter
from functools import cache
from itertools import product
from secrets import randbelow


VOLUNTEERS = "ABC"
START = frozenset({("", 0, False)})


@cache
def advance(states, volunteer):
    """Consume one visible symbol, collecting every possible private witness state."""
    next_states = set()
    for previous, reserves, selected_handoff in states:
        if volunteer == previous:
            continue
        next_reserves = reserves + (volunteer == "C")
        if next_reserves > 2:
            continue
        next_states.add((volunteer, next_reserves, selected_handoff))
        if previous == "A" and volunteer == "B" and not selected_handoff:
            next_states.add((volunteer, next_reserves, True))
    return frozenset(next_states)


@cache
def completions(states, shifts_left):
    if not states:
        return 0
    if shifts_left == 0:
        return int(any(selected for _, _, selected in states))
    return sum(completions(advance(states, who), shifts_left - 1) for who in VOLUNTEERS)


def roster_at(length, rank):
    if length < 0 or not 0 <= rank < completions(START, length):
        raise ValueError("Rank is outside the feasible roster population.")
    state = START
    roster = []
    for remaining in reversed(range(length)):
        for who in VOLUNTEERS:
            after = advance(state, who)
            count = completions(after, remaining)
            if rank < count:
                roster.append(who)
                state = after
                break
            rank -= count
    return "".join(roster)


def draw(length):
    if length < 0:
        raise ValueError("Roster length must be nonnegative.")
    count = completions(START, length)
    if count == 0:
        raise ValueError("This policy has no feasible roster of the requested length.")
    return roster_at(length, randbelow(count))


def witness_count(roster):
    """Independent whole-roster oracle; each A-to-B boundary may be the designated one."""
    if roster.count("C") > 2 or any(a == b for a, b in zip(roster, roster[1:])):
        return 0
    return sum(a == "A" and b == "B" for a, b in zip(roster, roster[1:]))


def oracle_rosters(length):
    """Brute-force every length-`length` string and keep the witness-bearing ones."""
    rosters = ("".join(xs) for xs in product(VOLUNTEERS, repeat=length))
    return {r: n for r in rosters if (n := witness_count(r))}


def check():
    checked = 0
    for length in range(10):
        wanted = oracle_rosters(length)
        total = completions(START, length)
        assert total == len(wanted)
        assert [roster_at(length, r) for r in range(total)] == sorted(wanted)
        for invalid in (-1, total):
            try:
                roster_at(length, invalid)
            except ValueError:
                pass
            else:
                raise AssertionError("Invalid rank accepted")
        if total == 0:
            try:
                draw(length)
            except ValueError:
                pass
            else:
                raise AssertionError("An impossible roster was drawn")
        if length == 8:
            print(f"8 shifts: {total} visible rosters, {sum(wanted.values())} witness-bearing assignments.")
            print("Rosters per witness multiplicity:", sorted(Counter(wanted.values()).items()))
            for distinct in sorted(set(wanted.values())):
                example = next(r for r, witnesses in wanted.items() if witnesses == distinct)
                print(f"{example}: {distinct} witness(es)")
        checked += 3 ** length
    print(f"Oracle examined {checked:,} candidate rosters across lengths 0-9.")


if __name__ == "__main__":
    check()
