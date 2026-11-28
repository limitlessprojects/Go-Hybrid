# Spike W-3060 — exact-value arithmetic: the silence-window census

**Status: pre-registered.** This file is committed in its own commit, **before**
the measurement is run; the numbers appear in `RESULTS.md`, added by the next
commit. That ordering is the point — the same protocol the repo already uses
for spikes.

**Answers:** wildcard proposal [#3060](https://github.com/limitlessprojects/Go-Hybrid/issues/3060)
— *flip SPEC §11 X1/X2/X4 from "silent wraparound" to "proved, or named"*, with
an exact-arithmetic reference interpreter as the artifact that outlives the
syntax.

## Hypothesis

SPEC §11 **X1** ("Signed integer overflow WRAPS (two's complement, defined)")
and **X2** ("Unsigned overflow wraps") define wraparound as *correct*, and the
checker's value-level machinery is the six `HYB-E6nnn` codes — all of which
inspect **compile-time constants** only. Therefore a typed-arithmetic
expression whose operands are **variables** is accepted with 0 diagnostics and
computed with two's-complement wraparound: a *silent wrong result* inside the
class BOUNTY.md already calls **Critical** ("wrong results in accepted code"),
for which the programme has no oracle.

**H1 — the window is not empty.** Of 10 hand-written typed-arithmetic shapes,
**≥ 8** diverge between X1/X2 wrapping and exact (arbitrary-precision)
arithmetic.

**H2 — the verdicts hinge on spelling.** All 10 *variables* shapes are
**accepted** by `hyb check` with `OK (0 diagnostics)`; their *constants* twins
are **rejected** with `HYB-E6002`. Same numbers, two spellings, opposite
verdicts — the claim-vs-reality hinge.

**H3 — falsifier.** If the measured divergence count is **0**, the premise of
#3060 is wrong and this spike *is* the M2 learning (paid either way). If any
*constants* twin is **accepted**, that is a sharper, separate finding — a hole
in the constant-overflow family itself — and will be reported on its own
rather than folded in here.

## Method (fixed before running)

- `exact_value_arithmetic.py`, standard library only, models
  (a) **X1/X2 wrapping** with explicit width truncation,
  (b) **exact** arithmetic (Python's `int` is arbitrary precision — that *is*
      the reference semantics), and
  (c) the truncated-toward-zero integer division/modulo Go's X4 uses.
- 10 shapes across `int8 int16 int32 int64 uint8 uint32 uint64 int uint`,
  mixing `+`, `-`, `*`, `<<` and one division/modulo case; each shape has a
  `vars` spelling and a `consts` twin.
- The script emits one `.hy` file per spelling into `cases/`, so every verdict
  is reproducible by anyone with the toolchain (`hyb check cases/<id>.hy`).
- `tests/test_exact_value_arithmetic.py` pins the **predicted** table below as
  unit tests, so the prediction is machine-checked rather than narrated.

## Pre-registered prediction (written before running)

| id | kind | expression | wrapped (X1/X2) | exact |
|----|------|------------|-----------------|-------|
| s1 | int8 | 100 + 100 | **-56** | 200 |
| s2 | int16 | 32767 + 1 | **-32768** | 32768 |
| s3 | int32 | 2000000000 + 2000000000 | **-294967296** | 4000000000 |
| s4 | int64 | 9223372036854775807 + 1 | **-9223372036854775808** | 9223372036854775808 |
| s5 | uint8 | 200 + 100 | **44** | 300 |
| s6 | uint32 | 4000000000 + 1000000000 | **705032704** | 5000000000 |
| s7 | uint64 | 18446744073709551615 + 1 | **0** | 18446744073709551616 |
| s8 | int8 | 100 * 2 | **-56** | 200 |
| s9 | int32 | 1 << 31 | **-2147483648** | 2147483648 |
| s10 | int8 | -128 - 1 | **127** | -129 |

## What this spike deliberately does NOT do

- It does **not** measure the corpus-wide *static-discharge rate* (fraction of
  sites provable `exact` with zero guards). That needs a real AST plus an
  interval pass and is pre-registered as M1 item 2 **in #3060**, not here.
- It does **not** propose that arithmetic trap at runtime. `wrapping` stays
  exactly today's X1; `no_wrap` is an opt-in type, per #3060.
- The "accepted" half is measured against a locally built `hyb` on `main`; the
  generated `cases/*.hy` are committed so the verdict is independently
  reproducible.
