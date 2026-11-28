# W-4851 Spike Results — Ordered Chunk Transformers for Serial Modular Recurrences

**Proposal:** [#4851](https://github.com/limitlessprojects/Go-Hybrid/issues/4851) |
**Author:** `@jimmy1008` | **Verifiers:**
`python3 spikes/w4851-ordered-chunk-transformers/chunk_transformer.py --check --live-go` &
`python3 -m unittest discover -s spikes/w4851-ordered-chunk-transformers/tests -p "test_*.py" -v`

The original submission modeled the algebra (`s -> A*s+B mod 2**w`,
associative/non-commutative composition) entirely in Python, against a
single fixed digit alphabet and modulus, with no source extraction and no
Go execution. This spike adds the two pieces it explicitly left open:

1. A **checked syntactic extractor** over literal `.hy`-shaped source that
   admits the narrow loop form and refuses everything else (state-dependent
   coefficient, opaque/effectful call, unrecognized loop body) — the
   fallback-to-serial path the proposal requires.
2. **Live Go code generation**: the extractor's own coefficient
   sub-expressions are spliced verbatim into a generated serial function
   and a generated ordered-chunk-transformer built from real goroutines,
   then built and run with the pinned-elsewhere Go toolchain — not just
   modeled in Python.

## 1. Pre-Registered Hypotheses & Declared Falsifiers (`P1`–`P4`, `P5`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **P1** | The extractor admits `s = A(input[i])*s + B(input[i])` for every word width (uint8/16/32/64) when `A`/`B` reference only `input[i]`, literals and `+ - * ( )`, and refuses a state-dependent coefficient, an opaque function call, and an unrecognized loop body (e.g. an early `break`) | `admits_narrow_form=True` for all 4 widths; `refuses_state_dependent`, `refuses_effectful_call`, `refuses_unrecognized_shape` all `True` | **PASS** |
| **P2** | Summarizing contiguous chunks as `(A,B)` and composing them in SOURCE order reproduces the serial reference evaluator exactly, over 400 randomized trials spanning all 4 widths, random coefficient functions, lengths and chunk counts (2–8 requested) | `ordered_matches=400/400` | **PASS** |
| **P3 (falsifier)** | Composing the same per-chunk summaries in a non-source order (here: reversed — the completion-order bug named in the proposal) diverges from the serial reference on the large majority of multi-chunk trials | `multi_chunk_trials=400`, `multi_chunk_mutant_mismatches=399/400` (the one survivor is a case where the composed coefficients happened to commute) | **FIRED** |
| **P4 (falsifier)** | A genuinely state-dependent recurrence (`s -> s*s`) cannot be admitted by probing two points (`s=0`, `s=1`) and pretending the result is affine; the probe's prediction at a third point diverges from the true recurrence, for every modulus tried | `(predicted, actual)` = `(2, 4)` for moduli `256, 65536, 2**32, 2**64, 1009` — mismatch in all 5 | **FIRED** |
| **P5 (live Go, `--live-go`)** | Go source generated from one admitted extraction — reusing its literal `a_expr`/`b_expr` text in both a serial function and a goroutine-based ordered-chunk-transformer — builds, runs, and matches the Python reference byte-for-byte across 60 randomized cases; a reversed-collector variant, generated and run the same way, diverges | `build_ok=True run_ok=True ordered_all_match=True mutant_mismatches=46/60` | **PASS / FIRED** |

Reproduce: `python3 spikes/w4851-ordered-chunk-transformers/chunk_transformer.py --check --json --live-go`.

## 2. What this confirms (and does not)

- **Confirms:** the admitted affine fragment composes correctly and
  order-sensitively across every word width Go actually has, not just the
  author's single `mod 256` example; a syntactic extractor can recognize
  the fragment and *refuse* every excluded shape named in the proposal
  (state-dependent coefficient, effectful call) without crashing — it
  falls back cleanly; and the same extracted expression text, spliced
  into real Go and executed, behaves identically to the Python model and
  to a correctly-ordered collector, while a collector that forgets source
  order is measurably wrong.
- **Does not confirm:** a real `.hy` AST-based extractor (this one is a
  bounded regex over one fixed loop shape, not the Rust checker), a
  parallel-scheduler cost model beyond the single `MIN_CHUNK_SIZE` gate
  used here, or any speedup claim — `chunk_transformer.py`'s Go harness
  runs goroutines but makes no timing claim, matching the original
  submission's own scope limit.

## 3. Relationship to #2385 (commutative merge capsules)

Unchanged from the proposal: these are genuinely different problems. The
`merge[M]` capsules in
[`spikes/w-commutative-merge-capsules-2385`](../w-commutative-merge-capsules-2385/)
require a **commutative** reducer and are explicitly allowed to combine in
completion order. This proposal's transform is **affine but not
commutative** (`P3` above is exactly the failure a commutative-merge
collector would hit if misapplied here), and the collector here is
required to use source order, never completion order. The two tracks are
complementary, not competing.

## 4. Next falsifiable step (unchanged from the proposal)

Recognize the restricted form in the real `.hy` AST (not a regex over
source text), emit the chunk tasks through `hyb build` instead of a
hand-written Go template, and compare against the real serial-path
codegen across the same parameter space. This spike's extractor already
pins the admission boundary (what must be refused) and its Go harness
already pins the correctness contract (ordered compose vs. the reference
evaluator); the remaining risk is exactly the one the proposal named:
exact extraction from the real AST.
