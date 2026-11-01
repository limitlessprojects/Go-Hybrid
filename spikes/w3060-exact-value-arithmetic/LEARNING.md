# LEARNING — Spike W-3060 (exact-value arithmetic), the published learning

Written for the Wildcard Track's M2 milestone (*"$250 when the learning is
published"* — `MOONSHOT.md`). The proposal is
[#3060](https://github.com/limitlessprojects/Go-Hybrid/issues/3060); everything
below is measured in this spike, and every prediction was committed **before**
its measurement (`PREREGISTRATION.md`, `BATTERY-PREREGISTRATION.md`,
`VALUE-ORACLE-PREREGISTRATION.md` — three separate commits).

## Proposal → Verdict → What we learned → Cost

### Proposal

Flip SPEC §11 **X1/X2/X4** from "wraparound is defined, so it is silent" to
**proved-or-named**: an arithmetic site becomes an effect (`exact` /
`guarded` / **named**), the qualifier lives in the type (`no_wrap` /
`wrapping` = today's X1 / `saturated`), a guard is emitted only where the proof
fails — and the artifact that survives even if the syntax is rejected: **a
second oracle, the exact-arithmetic reference interpreter**, so "wrong result in
accepted code" stops being unfalsifiable.

### Verdict

**Self-assessed: the premise is *confirmed*, the plan is *corrected*, and one of
my own predictions is *falsified*.** All three pre-registered falsifiers were
run; none killed the proposal, and one fired against me.

| # | pre-registered | measured | verdict |
|---|---|---|---|
| H1 | ≥ 8 of 10 typed-arithmetic shapes diverge between X1/X2 and exact arithmetic | **10/10 diverged; 10/10 predicted values exact** | held |
| H2 | the same numbers as **variables** accepted, as **constants** rejected | **10/10 accept (`OK`) vs 10/10 reject (`HYB-E6002`)** | held |
| M1-item-2 | discharge rate measured **on the corpus** | the corpus has **2** sites with both operand types declared — a denominator of two | **plan falsified → amended** |
| P1 | battery is not vacuous (≥ 1/3 diverging) | **75/192 = 39.1 %** | held |
| P2 | the trivial interval pass discharges **≥ 70 %** | **117/192 = 60.9 %** | **FALSIFIED (by me)** |
| P3 | every generated programme is inside the checked subset | **192/192 accepted** | held |
| O1 | the oracle reads the emitted text back faithfully, independently of the generator | **192/192 sites, 0 mismatches** | held |
| O2 | the divergence census is non-zero | **75** diverging sites | held |
| O3 | a Go cross-check exists and *skips* (never silently passes) without a toolchain | test present; skipped here, runs in CI | held |

The proposal's own defeat condition was **< 50 % static discharge**. The measured
number is **60.9 %**: above the floor, below my optimism. So the static-default
claim survives — with a number on the record instead of a belief.

### What we learned

1. **The claim was real, but the measuring instrument did not exist.** Hybrid's
   oracle stack is `go build` (`BUILDABLE`, Row 13) plus `-race`. Neither can see
   a value bug: `int8(100) + int8(100)` compiles, races nowhere, and prints
   `-56`. A programme can only pay for bugs its oracle can detect, and the class
   `BOUNTY.md` already calls **Critical** — *"wrong results in accepted code"* —
   had **no instrument at all** until this spike built one (`value_oracle.py`).
   That is the most reusable result here, independent of the syntax.
2. **The corpus cannot carry a value guarantee, and that is structural.** 130
   programs, **3174** arithmetic operator occurrences, but only **39** typed
   numeric declarations in **14** files and exactly **2** sites whose operand
   types are both declared. The corpus is a *concurrency* corpus: its arithmetic
   is inferred (`:=`) and therefore outside the width question. Any future value
   claim must bring its own battery — hence the amendment published on #3060, and
   hence the ordering that amendment fixes: **generator → analysis → oracle**.
3. **Static precision is worst exactly where the risk is worst.** Per operator
   the trivial interval pass discharges `%`, `>>`, `|` at **100 %** (they cannot
   overflow), `-`/`/` at 83 %, `+` at 17 %, `<<` at **4 %** and `*` at **0 %**.
   Multiplication and shifts are where guards would actually be paid — a concrete
   target for any real pass: **beat 0 % on `*`**.
4. **A pre-registered milestone can be structurally unmeasurable, and finding
   that out is itself the learning.** M1 item 2 looked reasonable in prose and
   was dead on arrival (denominator of two). The reconnaissance that exposed it
   was *lexical*, honestly labelled post-hoc, and published as an **amendment**
   rather than a quiet edit — the protocol working, not a flaw.
5. **Pre-registration is cheap and it changes the outcome.** Three hypotheses
   cost one extra commit each and made the results un-spinnable: my 60.9 % miss,
   the `same(1, 2.5)` overshoot I caught in the oracle while writing it, the
   constant-folded rows — every one is on the record with the number that decided
   it.

### Cost (honest)

- **What the spike cost:** ~2 sessions, **no checker or emitter change** (the
  spike is additive), 8 commits, 26 tests, 192 generated programmes, one
  interpreter and two censuses — all standard library, deterministic from a seed.
- **What dies if the proposal is ever implemented:** X1/X2 as the *default* for
  `no_wrap`-typed code; the constant-only boundary of `const_overflow.rs`; the
  single-oracle philosophy.
- **What must survive:** the corpus + `tools/corpus_harness.py` (still 130/130),
  the `--json` diagnostic contract, and rule FC (*"cannot prove ⇒ named"*), which
  the proposal reuses rather than bypasses.
- **Risks, unchanged from the proposal:** interval precision collapses on loops,
  aliases and foreign calls (what the 50 % bar measures); `i64::MIN/MAX` interval
  arithmetic needs care; guard cost in hot loops; floats are explicitly out of
  scope for M1 (rounding / NaN / ±0), which is why the defeat condition is stated
  for integers only.
- **Effort honesty:** 60.9 % is a **floor** — the pass is deliberately trivial
  (literal intervals only). A stronger pass raises it; the spike's job was to
  publish a number that cannot be spun in either direction.

## Reusable artifacts

| artifact | what it is |
|---|---|
| `value_oracle.py` | the **first value-level oracle** in the project: reads emitted `.hy` text, evaluates every arithmetic site in exact arithmetic **and** in what the emitted Go computes, and reports the divergence |
| `battery_generator.py` + `battery/` | a deterministic (seeded) narrow-width battery: 192 sites, boundary-weighted, each one a whole accepted programme |
| `corpus_numeric_census.py` | the sizing scan that showed the corpus has no typed narrow-width arithmetic to measure |
| `exact_value_arithmetic.py` + `cases/` | the silence-window census: variables vs constants, same numbers, opposite verdicts |

## Next step (for whoever continues)

The amended M1's first artifact exists (the generator). The next one is a **real
width+interval pass** measured against the same 192-site battery, and the number
to beat is published: **60.9 % static discharge, with `*` at 0 %**. The battery is
deterministic from `--seed`, so any improvement is directly comparable.

