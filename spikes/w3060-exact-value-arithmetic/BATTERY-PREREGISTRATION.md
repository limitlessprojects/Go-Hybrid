# Battery pre-registration — the narrow-width generator (M1 artifact 1 of #3060)

**Status: pre-registered.** This file is committed in its own commit, **before
`battery_generator.py` exists**. The numbers land in `BATTERY.md`, committed
afterwards — the same ordering the first spike used (`PREREGISTRATION.md`
`f8c93e7d`, then the results).

**Why a battery at all:** `CENSUS.md` showed the 130-program corpus cannot
carry the discharge-rate measurement — 3174 arithmetic operators across all 130
files, but only 39 typed numeric declarations in 14 files and exactly **2**
sites whose *both* operand types are declared. The amendment published on
#3060 therefore makes **the generator the first M1 artifact**; this is it.

## Sampling design (locked before the run)

Deterministic, seeded (`--seed`), no network, standard library only.

- **Grid:** 8 kinds `{i8, i16, i32, i64, u8, u16, u32, u64}` × 8 operators
  `{+, -, *, /, %, <<, >>, |}` × **3 draws** = **192 sites**.
- **Draws (boundary-weighted on purpose — a uniform sampler would almost never
  overflow):**
  1. **max-saturated**: both operands at the kind's maximum;
  2. **near-max**: one at maximum, the other a small positive delta;
  3. **uniform**: a seeded uniform draw inside the kind's range.
- **Divisor guard:** for `/` and `%` the divisor draw is forced non-zero, and
  the `min / -1` shape is included deliberately for the signed kinds — Go's
  spec makes that quotient wrap to `min` rather than panic, which is exactly the
  silent-wrong-result case this wildcard is about.
- Each site is emitted as a whole little program with the operands as typed
  **variables** (not constants — the whole point), plus a `fmt.println` so the
  result is used and the program is well-formed for `hyb check`.

## The analysis whose discharge rate is measured (locked before the run)

The trivial interval pass, computed *from the generated source's declared types
and literal initializers only* — never from the sampled ground truth:

- operand interval = the single point its literal initializer names;
- result interval = the operation applied in **exact** arithmetic (the same
  reference semantics as `exact_value_arithmetic.py`);
- verdict `exact` **iff** the whole result interval fits the declared kind's
  range; otherwise the site is `guarded` (a run-time check would be emitted) and
  counts **against** the rate;
- any site whose operand types could not be resolved counts **against** the rate
  (fail-closed accounting, never in favour).

## Pre-registered predictions

| # | prediction | falsifier if wrong |
|---|---|---|
| **P1** | boundary-weighted sampling makes the battery **non-vacuous**: at least **1/3** of the 192 sites have `wrapped != exact` | divergence share **== 0** ⇒ the generator is vacuous and the sampler is the wrong design |
| **P2** | the trivial interval pass discharges **≥ 70%** of sites as `exact` with **zero** emitted guard | **< 50%** ⇒ (the #3060 defeat condition) the *static-default* claim is dead and the proposal must fall back to opt-in `no_wrap` only |
| **P3** | every generated program is **accepted** by `hyb check` (`OK`, 0 diagnostics) — i.e. every site is inside the checked subset | any program **rejected** ⇒ the site shape is out of subset and must be re-scoped, not silently dropped |

## What this does NOT do

- No checker instrumentation: nothing here modifies `hy-check`; `hyb check` is
  used only to confirm acceptance (P3).
- No float arithmetic — integer kinds only, per the explicit non-goal in #3060.
- The pass is deliberately trivial (literals only). Its *weakness is the
  measurement*: a stronger pass would raise the rate, and this number is a
  floor, not a ceiling. That is stated here, before the run, so the number
  cannot be spun either way afterwards.
