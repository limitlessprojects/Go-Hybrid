# Spike Results: #4644 (Wildcard Track) — completion decided by the caller's observation

Model spike for issue #4644: a producer exposes a shrinking interval of possible
answers, a yes/no caller finishes once every remaining value answers the same, an
exact caller keeps going until the interval is a point.

Scope, before the run: interval bookkeeping over disjoint tiles, no task group, no
scheduler, no image. "Sound" here means: completion never happens while two allowed
values differ. All numbers are counts.

## Pre-registered hypotheses

| ID | Hypothesis | Falsified if |
|----|------------|--------------|
| H1 | Completion on `lo >= k` (at-least) or `hi < k` (below) is sound on every small tile set | one completion with disagreeing answers |
| H2 | The obvious shortcut — answer as soon as the midpoint answers — is unsound and often so | no such case found |
| H3 | A wide interval completes neither threshold claim | it completes |
| H4 | Each claim kind completes on its own condition only, and an exact claim never completes on an interval | cross-completion |
| H5 | Text and export can differ on the same count — the proposal's own example | they cannot |
| H6 | Refinement only tightens the bound | one widening |
| H7 | Empty, negative and inverted tiles are refused | silently accepted |

## Results

| ID | Gate | Result | Evidence |
|----|------|--------|----------|
| H1 | sound rule over the sweep | **PASS** | 1860 cases, 0 failures |
| H2 | midpoint rule is the losing one | **PASS** | 1860 cases, **910 unsound completions** |
| H3 | wide interval stays open | **PASS** | `0..10`, at-least 5 and below 5 both open |
| H4 | claim kinds are separated | **PASS** | at-least needs `lo >= k`, below needs `hi < k`, exact only at `lo == hi` |
| H5 | preview and export diverge | **PASS** | tile `600..10^6`: at-least 500 completes, exact does not |
| H6 | refinement monotone | **PASS** | 306 refinements, 0 widenings |
| H7 | bad tiles refused | **PASS** | `HYB-E4644-02` negative, `HYB-E4644-01` inverted and disjoint |

11 tests, all green.

## What the numbers say

The shortcut that a working implementation would reach for first — stop when the
midpoint answers — is wrong in **49 %** of the swept cases (910/1860). The sound rule
is wrong in none. That gap is the whole point of the proposal: the completion
condition is a property of the *caller's* question, not of the count, and a producer
that guesses an answer instead of proving one saves nothing it can keep.

A weaker but real finding: the exact export never completes early in this model, so the
"two algorithms" the issue wants to avoid do fuse — but only as long as the producer
keeps refining. A tile whose bound no longer moves strands the exact caller at an open
interval, and the model has no timeout to notice. That is the next thing to model, not
a solved property.

## What this does not show

- **No task group.** Nothing is cancelled, no worker joins; completion is a predicate
  over bookkeeping.
- **No producer.** The spy assumes every tile can be refined downward; a real counter
  may reach a tile it cannot bound better (regex match counts, sampled bytes), and then
  the at-least claim never completes.
- **Intervals are exact integers per tile.** No correlation between tiles, no shared
  bounds, and no cost model — counting one pixel more is free here.
- **`EXACT` returns `True` as its answer**, which is only a placeholder: an exact caller
  wants the number, so `point_value()` exists and no claim consumes it.
- **No write path.** The image is read-only in the model; the proposal's import case is
  not covered.

## Files

- `completion_interval.py` — tiles, claims, sound and midpoint completion, monotone
  refinement, the sweeps.
- `tests/test_w4644_spike.py` — H1–H7 as gates.
