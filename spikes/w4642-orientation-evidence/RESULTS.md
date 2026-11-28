# Spike Results: #4642 (Wildcard Track) — orientation evidence bound to its sites

Model spike for issue #4642: `observe(mesh, i, j, k)` borrows a coordinate snapshot,
`orientation(observation)` returns a turn with the evidence attached, and a checked
insert consumes it only for that mesh revision, that ordered triple and those
coordinates.

Scope, before the run: Python model, integer coordinates bounded by 2^50, no backend,
no mesh data structure beyond a coordinate dict. All numbers are counts.

## Pre-registered hypotheses

| ID | Hypothesis | Falsified if |
|----|------------|--------------|
| H1a | Exact integer orientation over differences up to 2^51 is correct; no wrap, no rounding | one wrong sign |
| H1b | float64 orientation of the same formula is unreliable at the proposed 2^50 bound — that is, the binding guards against numeric error | zero float failures in 20000 near-collinear cases |
| H2 | Reordering the triple is rejected as a mismatch, and the sign differs for the two orders, so a bare sign cannot tell them apart | accepted, or equal signs |
| H3 | Evidence from another mesh is rejected | accepted |
| H4 | A coordinate edited after the observation is rejected — by revision, and by digest if the revision was not bumped | accepted |
| H5 | A forged turn text is rejected | accepted |
| H6 | Coordinates above 2^50 fall outside the fragment and need an escape | silently accepted |

## Results

| ID | Gate | Result | Evidence |
|----|------|--------|----------|
| H1a | exact signs | **PASS** | hand turns ±1/0 at the bound, exact for 20000 cases |
| H1b | float64 unreliable at 2^50 | **PARTIAL — falsified at 2^50** | 0/20000 wrong at 2^50, also 0/20000 at 2^51; first failures at 2^52 (26/20000) and 2^53 (346/20000) |
| H2 | reorder rejected | **PASS** | `HYB-E4642-03`, and the two orders yield opposite turns |
| H3 | other mesh rejected | **PASS** | `HYB-E4642-02` |
| H4 | moved vertex rejected | **PASS** | `HYB-E4642-01` on revision, `HYB-E4642-04` on unseen coordinate change |
| H5 | forged turn rejected | **PASS** | `HYB-E4642-05` |
| H6 | above-bound coordinate rejected | **PASS** | `HYB-E4642-06` |

11 tests, all green.

## The finding that matters

**The proposal's 2^50 bound is numerically conservative.** Differences of ≤ 2^50
integers stay exactly representable, and the products only lose bits far below the
unit offset that a degenerate triple contributes: at 2^50 and 2^51 no float64 sign
was wrong in 20000 near-collinear cases each, and failures only start one and two bits
higher. So the arithmetic is not what the binding buys at this bound — a robustness
library would hand out correct signs.

What the binding buys is what the issue actually argues: **association**. Same sign,
different meaning. A permutation of the triple flips the sign (H2), an edited
coordinate keeps the old sign valid while the site has moved (H4), and the same
coordinates in another mesh authorize a write elsewhere (H3). None of that is visible
in a bare sign, and all of it is caught here.

A second, unwelcome result: raising the bound to 2^52 is not free. Anyone who widens
the fragment to fit real meshes inherits float64's boundary and then needs an exact
fallback (Shewchuk-style adaptive arithmetic or a big-integer predicate like this
model).

## What this does not show

- **No real mesh type.** `insert_face` bumps a revision and returns the triple; it does
  not write topology, check edge existence or handle boundary cases.
- **No slice-through-unverified-Go test.** `HYB-E4642-06` only marks the bound; what a
  caller loses when it builds a turn outside the checker is unmodelled.
- **Digest is not part of the proposal.** It is this spike's device for catching a
  coordinate change that forgot to bump the revision; a real compiler pass would query
  the source snapshot instead.
- **No timings.** Whether carrying evidence costs anything is unmeasured.
- Collinear output is a valid turn; the proposal does not say whether an insert may
  consume one, so the model allows it and does not test it.

## Files

- `orientation_evidence.py` — exact and float predicates, evidence, certification,
  failure-rate measurement with two generators.
- `tests/test_w4642_spike.py` — H1–H6 as gates.
