# Value-oracle pre-registration — the second oracle (M2 artifact of #3060)

**Status: pre-registered.** Committed in its own commit, **before
`value_oracle.py` exists**. Results land in `ORACLE.md`, committed after.

**Why this artifact exists:** #3060's whole claim is that a program Hybrid
*accepts* can silently compute a wrong value, and that the programme has **no
oracle that can see one** — `go build` compiles it, `-race` is silent. The
proposal calls the exact-arithmetic reference interpreter *"the piece that
matters even if the syntax is rejected"*. This is it, in miniature.

## What the oracle is

An interpreter for the narrow subset the battery and the cases actually use
(`var <name> <kind> = <int>`, `c := a <op> b`, `_ = a <op> b`, `fmt.println`),
which reads the **emitted `.hy` source text** and evaluates every arithmetic
site **twice**:

- **`exact`** — arbitrary-precision arithmetic (Python `int`), the reference
  semantics;
- **`x12`** — two's-complement wraparound at the declared width, which is what
  the *emitted Go* computes (SPEC §11 X1/X2).

A site where the two disagree is a **silent wrong result inside an accepted
program**: the checker accepted it, Go builds it, and the value is not the one
the source names.

**Honest scope:** the reader is a regex-level parser for that subset, not a
`.hy` front end. That limitation is the reason for O1 below — the claim that it
reads the emitted text *faithfully* is measured, not asserted.

## Pre-registered predictions

| # | prediction | falsifier if wrong |
|---|---|---|
| **O1** | the oracle, reading the emitted source **independently of the generator's in-memory table**, reproduces that table on **192/192** battery sites (same `exact`, same `x12`, same divergence flag) | any disagreement ⇒ the reader is not faithful and every count below is void |
| **O2** | the divergence census over `battery/` + `cases/` is **non-zero** — i.e. accepted programmes whose emitted Go computes a value other than the exact one exist | **zero divergence ⇒ the premise of #3060 is falsified** (this is H3 from `PREREGISTRATION.md`, restated for the oracle) |
| **O3** | a Go cross-check test exists that compares the oracle's `x12` model against the **real emitted Go** when a Go toolchain is present, and **skips** (never silently passes) when it is not | the test passes without Go, or asserts nothing |

O1 is the load-bearing one: without it, O2 would only be restating the
generator's own arithmetic with extra steps.

## Method (fixed before the run)

- `value_oracle.py`, standard library only, no network.
- Runs over a directory of `.hy` files (`--dir`), defaulting to `battery/`.
- Emits `ORACLE.md`: the per-file table for a sample, the aggregate census, and
  the three verdicts above.
- The Go cross-check lives in the test suite, guarded by `shutil.which("go")`.

## What this does NOT do

- It is **not** a `.hy` front end and does not claim to cover the language; only
  the subset the battery and cases use.
- It does **not** prove anything about programs outside that subset — the
  divergence *rate* here is a property of the battery's shapes, not a
  population estimate.
- It does not modify the checker or the emitter.
