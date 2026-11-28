# Spike Results: #4685 (Wildcard Track) — run-domain emit target

Model spike for issue #4685: a checked fragment that stays in the run domain, so a
source already stored as (value, repeat count) is reduced by one predicate evaluation
per run instead of one per logical sample.

Scope, stated before the run: this is a Python model plus emitted Go for the loop
only. There is no Hybrid backend, no compiler pass, no throughput measurement. Every
number below is a count, not a timing. Proposals say the arithmetic idea is checkable
without a backend; that is exactly what is checked, and nothing more.

## Pre-registered hypotheses

| ID | Hypothesis | Falsified if |
|----|------------|--------------|
| H1 | Dense count equals run-domain count on every sequence of length 0..7 over {0,1,2} with `(x*x) % 5 == 1` (3280 sequences) | one mismatch |
| H2 | The count is additive under splitting at every position (24604 splits) | one mismatch |
| H3 | A single run of 10^9 logical samples costs one predicate evaluation and yields the dense count | more than one evaluation, or wrong count |
| H4 | Admission rejects zero length, uint64 logical-length overflow (even when the predicate is false), int64 sample range, time reads, position predicates, modulo by zero | any case admitted silently |
| H5 | The emitted Go loop is syntactically valid, gofmt-clean, compiles, and counts like the model | gofmt diff, build error, deviating count |
| H6 | The run domain does not expand valid runs: allocation follows the run count, not the logical length | run allocation tracks logical length |

## Results

| ID | Gate | Result | Evidence |
|----|------|--------|----------|
| H1 | dense ≡ run domain, 3280 sequences | **PASS** | `check_equivalence` → `mismatches=0` |
| H2 | split additivity, 24604 splits | **PASS** | same run, `mismatches=0` |
| H3 | 1 run × 10^9 | **PASS** | `predicate_evals=1`, `total=1000000000` |
| H4 | admission matrix, 7 rejects | **PASS** | `HYB-E4685-01` zero length, `-02` single and total overflow (predicate `0`), `-03` sample ≥ 2^63, `-04` time, `-05` position, `-06` modulo by zero |
| H5 | emitted loop is real Go | **PASS** | gofmt `-l` empty, `go run` builds, count 4 = dense reference, evals 3 = run count |
| H6 | no expansion | **PASS (model level)** | `run_alloc=1` vs `dense_alloc=1000000000`; alternating runs give `predicate_evals == 1 per sample`, no advantage |

13 tests, all green: `python3 -m unittest discover -s tests`.

## What this does not show

- **No timings.** H6 is allocation order and evaluation counts; whether the run loop
  is *faster* on a real Hybrid backend is unmeasured. Alternating inputs must lose or
  draw, and the proposal expects that.
- **No compiler work.** Admission is a hand-built AST check, not a pass over Hybrid's
  IR. Time and position reads are rejected because the model has no second input,
  which is the honest structural reason, not a modelled analysis.
- **Modulo by zero is found at runtime**, not at compile time. A value-range analysis
  could move it to compile time; that is not in this fragment.
- **Sample arithmetic wraps at int64** on purpose (`_i64`), so overflow inside the
  predicate is Go-identical. Division is not in the fragment: Go's `MIN_INT64 / -1`
  traps, the model would not.
- The 10^9 case never materialises 10^9 elements in Python either; `dense_alloc` is the
  logical length, an arithmetic figure.
- `Length` is `uint64` while the sum is checked against `uint64` as well; the proposal
  requires exactly that, but a counting result that fits `uint64` still cannot be
  handed to code expecting `int64`.

## Files

- `run_domain_emitter.py` — fragment AST, admission, run-domain counter, dense
  reference, Go emitter, equivalence harness.
- `tests/test_w4685_spike.py` — H1–H6 as gates, Go test skipped when the toolchain is
  absent.

Run:

```
python3 run_domain_emitter.py
python3 -m unittest discover -s tests
```
