# Spike Results: Issue #4807

**Proposal:** [#4807](https://github.com/limitlessprojects/Go-Hybrid/issues/4807)
— *Compile recognizer revisions into the smallest test suite that tells
them apart* (Wildcard Track). **Verifier:** `python3 -m unittest discover
-s spikes/w-recognizer-equivalence-4807/tests -p "test_*.py" -v`

## Summary

Implementation of the two techniques the proposal names — pairwise
automaton equivalence and minimum test cover — as a standalone module
(`recognizer_equivalence.py`), validated against the two fixtures the
proposal's own prose describes. No prototype was required for this track;
this spike provides one anyway, scoped to the "standalone model" stage the
proposal itself says it has reached. Extracting transitions from real
`.hy` source and running witnesses against separately built Go binaries
is explicitly the proposal's *next* experiment, not this one.

## What's implemented

- `DFA`: a deterministic, total recognizer over an explicit `(state,
  symbol) -> state` table; any missing edge is an implicit transition to
  a shared dead/reject sink, so every recognizer is total without writing
  out every rejecting edge.
- `joint_bfs`: breadth-first search of the *product* automaton of all
  candidate revisions together. Because every revision's state space is
  finite, the product is finite too, so this is an exact equivalence
  check, not a length-bounded approximation — no joint state is skipped,
  and every reachable answer vector gets its shortest (then
  lexicographically-least) witness string.
- `equivalence_classes`: unions revision indices whenever no reachable
  product state ever gives them different accept bits.
- `minimum_test_cover`: exact minimum set cover over "pairs of revisions
  this witness separates," searched by brute force over increasing subset
  sizes (the instances this track targets are small: a handful of
  revisions, a handful of distinct answer vectors). Past a configurable
  `budget` of combinations tried, it falls back to a greedy cover and
  marks the result `exact: False, upper_bound: True` instead of silently
  returning a non-optimal cover as if it were minimal — the cost-honesty
  case the proposal asks for.
- `classify`: matches a black-box implementation's answers on the chosen
  pack against known class vectors, returning `None` ("unknown") when the
  answers match zero or more-than-one known class.

## Fixture 1 — eight number grammars, three probes

Eight recognizers for number literals, gated by three independent flags
(`allow_plus`, `allow_leading_zero`, `allow_underscore`). By construction,
`"+0"` is accepted iff `allow_plus`, `"00"` iff `allow_leading_zero`, and
`"1_0"` iff `allow_underscore` — each probe isolates exactly one flag,
regardless of the other two.

| Check | Result |
|---|---|
| All 8 revisions pairwise distinguishable | **28/28 pairs** (`test_eight_versions_all_pairwise_distinguishable`) |
| Minimum test cover | **exact, size 3**: `{"+0", "00", "1_0"}` (`test_minimum_cover_is_exactly_the_three_probes`) |
| Any 2-probe subset of the cover | **insufficient** — 2 bits cannot injectively label 8 revisions (`test_cover_is_valid_and_two_probes_are_not_enough`) |
| `classify` on each class's own answers | **identifies every class correctly** (`test_classify_identifies_each_known_class`) |

This reproduces the proposal's own claim ("Eight small number grammars in
my fixture need just three requests: `+0`, `00` and `1_0`. Each version
gives a different three-bit answer") from an independent construction,
not by copying undisclosed fixture internals.

## Fixture 2 — four tags, one baseline, one equivalent lookalike

Four revisions each accept exactly one one-byte tag (`A`/`B`/`C`/`D`); a
baseline accepts nothing; a sixth revision also accepts nothing but
reaches that behavior by oscillating between two non-accepting states.

| Check | Result |
|---|---|
| Baseline and toggling lookalike share a class | **yes** — no reachable product state separates them (`test_baseline_and_toggler_share_a_class`) |
| Equivalence classes | **5** (`{baseline, toggler}`, `{A}`, `{B}`, `{C}`, `{D}`) |
| Minimum test cover | **exact, size 4**: `{"A", "B", "C", "D"}` (`test_minimum_cover_needs_all_four_tags`) |
| Dropping any one tag probe | **breaks separation** of that tag's revision from the baseline class (`test_each_tag_is_individually_necessary`) |
| `classify` on an unmodeled lookalike that only agrees with tag-A *on the pack* | **matches tag-A's class** — correctly non-falsified by the pack, even though a probe outside the pack's alphabet (`"AZ"`) does tell them apart (`test_unmodeled_implementation_can_match_a_class_on_the_pack_alone`) |
| `classify` on an answer no modeled revision gives | **`None` (unknown)** (`test_classify_reports_unknown_for_an_answer_no_class_gives`) |

This reproduces the proposal's claim that the minimum cover is four
"despite there being only five distinct behaviors," because each tag
probe is the *only* thing separating its revision from the baseline
class, and confirms the "arbitrary unmodeled implementation can give the
same answers on the pack" caveat as an expected, tested outcome rather
than an unverified assertion.

## Cost honesty

`minimum_test_cover`'s exact search is exponential in the number of
distinct answer vectors; `TestCostHonesty` forces `budget=1` on the tag
fixture and confirms the result is returned as `exact: False,
upper_bound: True` rather than as a false optimum, and that a realistic
budget (`200_000`, the default) recovers the true minimum for both
fixtures.

## Scope / next steps

Per the proposal's own "cost honesty" section, this stays limited to
deterministic, total recognizers with an explicit finite state space
built by hand in Python. The proposal's named next experiment — extract
transitions from actual `.hy` source and run each emitted witness against
separately built Go binaries — is out of scope here and remains future
work.
