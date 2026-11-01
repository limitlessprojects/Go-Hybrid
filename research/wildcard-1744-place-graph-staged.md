# Wildcard #1744 — The place graph is right; the 350-LoC estimate is off by ~5× and the closure story is missing

**Issue:** [#1744](https://github.com/limitlessprojects/Go-Hybrid/issues/1744) · **Status:** design-only; no checker changes in this PR.

## 0. Verdict up front

The diagnosis is correct and worth acting on: `hy-check` runs ownership, loan, handle-consume and sendability rules directly over the surface AST, and every new wrapper form adds another pattern-match branch. The proposed remedy — a capability-carrying place-graph IR — is the right shape of fix. The **cost estimate is not**. "~350 LoC" survives only if the pass is scoped to parenthesised expressions, and parens are a rounding error in the actual coupling. Measured here: **17 `Expr::Paren` sites** across the six rule files versus **676 `Expr::` match sites** and **220 closure references**. A pass that only erases wrappers removes 17 branches; a pass that canonicalises places has to handle closures, and closures are where the real work is.

What survives is a **staged** lowering: wrapper normalisation first (small, provable, immediate), place canonicalisation second (the bulk of the value), closure capture last (its own project). Each stage is independently falsifiable, and the first two together get most of the soundness win without waiting for a full MIR.

## 1. The coupling, measured

| File | `Expr::` sites | `Expr::Paren` | Closure refs |
|---|---|---|---|
| `sendability.rs` | 182 | 2 | 37 |
| `loans.rs` | 229 | 6 | 98 |
| `walk.rs` | 70 | 5 | 9 |
| `buildable.rs` | 161 | 3 | 12 |
| `closures.rs` | 24 | 1 | 63 |
| `freeze.rs` | 10 | 0 | 1 |
| **Total** | **676** | **17** | **220** |

Two conclusions the proposal should absorb:

1. **Parens are not the problem.** 17 of 676 sites. Even if `Expr::Paren` were erased tomorrow, 97.5% of the surface coupling remains. The header claim "zero syntax wrappers by construction" is true but small.
2. **Closures are the problem.** 220 references, concentrated in `loans.rs` (98) and `closures.rs` (63). A closure body is not a wrapper that can be stripped — it captures places, and a place graph must model *which* place each capture aliases. That is a capture analysis, not a lowering convenience.

## 2. Why "350 LoC" cannot hold

A place-graph IR that actually covers the rule surface needs, at minimum:

- `Place { root: BindingId, projections: Vec<Proj> }` with `Proj ∈ {Field, Index, Deref, Slice}` and stable identity across lowering — the proposal's own core.
- Rvalue materialisation rules so that `wrap{h: h}.h.doc` becomes a place chain rather than a field-hop the rules re-derive.
- Auto-addressed receiver handling (6 sites today) — receiver places are created implicitly by method syntax and must become explicit.
- Tuple assignment / multi-value desugaring, because `walk.rs` branches on `Stmt` shapes that disappear at MIR level.
- Capture places for 220 closure sites: for each closure, the set of outer places it can read or move. Without this, `loans.rs` cannot decide invalidation, which is exactly the bug class in #1792 ("A3 map-slot loan invalidation misses receiver mutations via helper").
- A mapping back to source spans, or every diagnostic loses its location.

Realistically that is **~1,800–2,500 LoC** and two to three weeks of review, before any rule is migrated. The value is real; the schedule in the proposal is not.

## 3. Staged plan (each stage independently shippable)

**Stage 1 — Wrapper normalisation (~300 LoC).** Erase `Expr::Paren`, desugar `some(e)` where it is purely syntactic sugar for the `Option` constructor at the AST level, normalise auto-addressed receivers into explicit `&place`, and flatten composite field hops into explicit temporaries. No new IR — a `normalize(Expr) -> Expr` pass with a corpus-wide golden test. Value: removes 17 paren branches and the field-hop class; costs almost nothing; falsifiable in a day.

**Stage 2 — Place canonicalisation (~1,200 LoC).** Introduce `Place`/`Proj` and rewrite `as_addr_of`, `classify_value`, `collect_move_operands`, and `check_o54_non_ident_move` to consume places. This is where #1775 (address-of on index/slice of a map-slot projection) and #1822 (`name == "pop"` hijacking method resolution) actually die. Closures are left on the AST path and treated conservatively (any capture of a place that escapes a loan window is rejected).

**Stage 3 — Capture places (~800 LoC).** Only if stage 2 shows measurable gains; model closure captures as places and relax the conservative rule.

The proposal bundles all three into one pass and one number. Splitting them keeps the soundness wins coming while the hard part is designed.

## 4. Pre-registered tests

1. **Corpus parity at every stage.** All 129 files produce byte-identical diagnostics; stage 1 additionally requires identical emitted Go (it is a pure normalisation).
2. **Wrapper-class falsifier.** Count diagnostics that change when `Expr::Paren` is erased: if stage 1 fixes zero corpus cases and zero filed issues, the wrapper class was already dead and only stage 2 matters.
3. **Place-class wins.** After stage 2, #1775 and #1822 reproducers from their issues must be rejected with a span pointing at the offending projection. Two independent bugs, one mechanism — that is the point.
4. **Closure conservatism cost.** Measure how many corpus programs stage 2 would reject because a capture escapes a loan window. If > 5%, stage 2 cannot ship without stage 3 and the plan must say so up front.
5. **LoC honesty.** Report actual added/changed lines per stage against the 300 / 1,200 / 800 estimates; a stage that exceeds its budget by > 30% triggers a design review before merge.

## 5. What does not survive

- **"~350 LoC"** for the whole thing — the closure capture analysis alone is larger, and the rule migration touches 676 match sites.
- **"Zero syntax wrappers by construction" as the headline win** — it is 17 of 676 sites; the win is place identity, not wrapper erasure.
- **Single-stage migration** — it front-loads the hardest part (closures) and delays every measurable benefit behind it.

**What survives:** the place graph, delivered in three provable steps, with the closure capture problem named instead of assumed away.
