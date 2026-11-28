# Spike Results — Fair Roster Draw From a Validator With Hidden Choices (#4896)

## Claim under test

A coordinator publishes an 8-shift roster over volunteers A/B/C (nobody
serves twice in a row, C serves at most twice) and separately designates one
A-to-B handoff as the training slot. Drawing uniformly over *roster-and-
handoff pairs* is unfair to rosters with few handoff choices: `ABABABAB` has
four eligible handoffs and `ABACACBA` has only one, so a pair-uniform draw
gives the former four times the odds of the latter. The proposal: determinize
the hidden handoff-selection state, count accepting suffixes from each
reachable state set with arbitrary-precision integers, and rank-decode a
draw so every *roster* — not every pair — gets exactly one ticket.

## What was verified

`roster_draw.py` implements the model from the issue; `tests/test_w4896_spike.py`
checks it against an independent brute-force oracle (`oracle_rosters`, which
enumerates every string over `{A,B,C}` and keeps the ones with no repeat and
at most two `C`s, counting their handoff multiplicity directly).

| Check | Result |
|---|---|
| 8-shift visible roster count | **151**, matching the issue |
| 8-shift witness-bearing (roster, handoff) pairs | **284**, matching the issue |
| Rosters per handoff multiplicity | `1→43, 2→84, 3→23, 4→1` |
| Population by length 0–9 | `0, 0, 1, 4, 11, 27, 58, 102, 151, 204` |
| Rank-decode vs. oracle, every length 0–9 | identical lexicographic list at every length (29,524 candidate rosters examined) |
| Out-of-range rank (`-1`, `== total`) | rejected with `ValueError` |
| Empty population (lengths 0 and 1 — too short to contain an A→B boundary) | `draw()` rejects with `ValueError` instead of returning a result |
| Negative length | rejected with `ValueError` |
| 200 live draws via `secrets.randbelow` | every result lands in the oracle's accepted set |

9/9 tests pass (`python3 -m unittest discover -s tests -p "test_*.py"`).

## Scope note

This spike covers the model only, as the issue's cost-honesty section
flags: source extraction from a real `.hy` policy into emitted Go
count/rank-decode functions (the M1 milestone) is not attempted here. The
reachable-state-set determinization this relies on can grow exponentially
in general; the issue's proposed caps (4,096 reachable subsets, 64 MiB of
count storage) are not yet enforced by this spike and would need to gate
any extraction step.

## Reproduction

```bash
cd spikes/w-roster-draw-hidden-choices-4896
python3 roster_draw.py                              # prints the self-check summary above
python3 -m unittest discover -s tests -p "test_*.py" -v
```
