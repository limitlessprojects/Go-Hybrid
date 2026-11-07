# Wildcard #1685 — Cap-clipped reslices, with the type oracle the idea needs

**Issue:** [#1685](https://github.com/limitlessprojects/Go-Hybrid/issues/1685)

**Verdict up front:** the direction is right and the P0 family is real, but the
proposal as written — "every 2-index slice `s[lo:hi]` lowers to
`s[lo:hi:hi]`" — cannot ship as a purely syntactic emitter rule. Hybrid strings
are sliceable (`check_slice_kind` admits `OpKind::Str`,
`compiler/crates/hy-check/src/buildable.rs:3365`), and Go forbids 3-index
strings. `tests/checker-corpus/14-a7-use-after-consuming.hy:52` slices the
const string `payload` in a loop; the blanket rule would emit
`payload[a:b:b]` and fail with `invalid operation: 3-index slice of string`.
The emitter also has no general type oracle: `type_of_expr`
(`compiler/crates/hy-emit/src/expr.rs:567`) resolves only its shape/foreign
descriptor tables, not ordinary locals. What survives is a checker-planned clip
that knows which bases are slices/arrays, plus a helper that evaluates each
bound once.

## 1. The mechanism

### Today

- The AST has only `lo`/`hi` (`compiler/crates/hy-syntax/src/ast.rs:544`); the
  parser builds that node (`compiler/crates/hy-syntax/src/parser.rs:2264`); the
  language spec still marks 3-index as `[TBD]` (`SPEC-LANGUAGE.md:386`).
- The emitter prints `base[lo:hi]` verbatim
  (`compiler/crates/hy-emit/src/expr.rs:190`), so Go's capacity rule
  (`cap = cap(base) - lo`) leaks through unchanged.
- The checker validates the base kind (`buildable.rs:3365`) but only produces
  diagnostics; `hyb` calls `hy_check::check_file` and then `hy_emit` with no
  type handoff (`compiler/crates/hyb/src/main.rs:512`, `:582`).

### Proposed

1. **Plan, don't guess.** During `check_buildable`, record every `Expr::Slice`
   whose base kind is `Slice`, `Array`, or pointer-to-array in a `ClipPlan`
   carried on `CheckResult`; leave `Str` and `Unknown` out. `hyb` passes the
   plan to `emit_go_package`. This reuses the checker's existing
   `kind_of_expr` (`buildable.rs:1818`), which already resolves locals, calls,
   fields, and literals — the oracle the emitter lacks.
2. **Lower clipped views through prelude helpers**, not duplicated syntax:
   - `s[lo:hi]` → `hy_slice2(s, lo, hi)`, where
     `func hy_slice2[T any](s []T, lo, hi int) []T { return s[lo:hi:hi] }`.
   - `s[lo:]` / `s[:]` → `hy_slice_open(s, lo)`, returning
     `s[lo:len(s):len(s)]` (or `s[:len(s):len(s)]` for `s[:]`).
   - arrays → pass `a[:]` (the plan marks array), so the helper receives a
     slice with `cap == len(a)`.
   - explicit 3-index `s[lo:hi:max]` → direct Go 3-index; each operand is
     evaluated once.

   The helper matters: `s[lo:hi:hi]` evaluates `hi` twice and changes
   side-effects (`s[0:f()]` would call `f` twice). A one-line generic helper
   keeps one evaluation and expression-local lowering without adding let
   insertion to `emit_expr`. `hy_pop` is the existing prelude precedent
   (`compiler/crates/hy-emit/src/lib.rs:110`).
3. **Cap rule.** Every clipped 2-index view has `cap == len` by construction:
   `cap == hi - lo` for `s[lo:hi]`, `len(s) - lo` for `s[lo:]`, and `len(s)`
   for `s[:]`. Explicit `s[lo:hi:max]` restores `cap = max - lo`;
   `make([]T, n, c)` remains the owned spare-capacity constructor. Strings keep
   2-index and are unaffected (there is no `append` on strings).
4. **Diagnostics.** A constant `max < hi`, or a constant `max > cap(base)` where
   the checker can fold `cap(base)`, reuses `HYB-E1002`, the same fail-closed
   family `check_slice_kind` already uses for non-sliceable bases and
   non-integer bounds; 3-index on a string, or on a base the plan cannot prove
   is a slice/array, is `HYB-E1002` too (the emitter cannot tell a slice from a
   string, so it must not guess). Dynamic violations remain Go runtime panics,
   exactly as 2-index bounds do today. Default clipping emits no diagnostic: it
   is a language-version change, not an error.

## 2. What it fixes

- The five numbered #1683 reproducers (six programs): `s[:1]` becomes
  `cap == 1`, so `append` must allocate and `s[1]` is never written. The same
  applies to `put(s[:1], m)`, the multi-return form, the const form, and the
  map-slot loan form.
- The #560/#773 bounded-reslice const bypasses (`primes[:1]`,
  `holder{xs: primes[:1]}`, `grid[0]`), because the destination view has no
  spare capacity.
- Array reslices, including `s := a[:]`
  (`tests/checker-corpus/53-array-slice-backing-freeze.hy:30`).
- It is physical, not checker-dependent: even if `loans.rs` misses a new append
  path, runtime reallocation prevents the parent-tail write.

## 3. What it does not fix, and what it breaks

- **Make-cap and foreign slices.** `shared := make([]int, 1, 4);
  alias := append(shared, 1)`
  (`tests/checker-corpus/append-spare-capacity-backing-alias.hy:40`) still
  aliases; this proposal removes only *reslice-derived* ambient capacity. The
  issue's "checker deletion" claim is therefore overstated. `record_may_share`
  (`compiler/crates/hy-check/src/loans.rs:2359`) must still track reslice
  aliasing (`s2 := s1[:]` shares elements), `check_delete_const`/
  `append_dst_reallocates` (`loans.rs:6351`, `:6374`) still need make/foreign
  cases, `call_returns_fresh_copy`
  (`compiler/crates/hy-check/src/freeze.rs:1019`) still distinguishes
  value-element append returns, and `block_returns_only_fresh_append` /
  `is_fresh_empty_slice_append` (`freeze.rs:1097`, `:1201`) remain. Expect a
  modest deletion, not a rewrite of `loans.rs`/`freeze.rs`.
- **String slices** are the fatal counterexample to a syntactic rule; the plan
  handles them by not clipping.
- **Unknown bases** (generic type parameters, opaque FFI results) stay 2-index;
  the guarantee is scoped to plan-covered views. P5 measures coverage; if
  P0-shaped code is `Unknown`, the proposal fails its own test.
- **`cap` is observable.** `tests/checker-corpus/51-make-slice-cap.hy:34`
  prints it; `buf = buf[:0]` loses capacity and subsequent appends reallocate.
  Migration needs `buf = buf[:0:cap(buf)]` (or a new `buf.reset()` intrinsic)
  before flipping the default.
- **Silent semantic change:** append through a reslice no longer mutates the
  parent tail. That is the point, but code relying on it breaks without a
  diagnostic; release notes and a compat flag are required.
- **Grammar/API cost:** 3-index is `[TBD]` (`SPEC-LANGUAGE.md:386`); adding
  `max` to `Expr::Slice` touches the parser and every match site. W-015's open
  spike #379 already prototypes this for `split`; reuse it rather than fork.
  `hy-emit`'s public signature and `CheckResult` change; the golden
  `slice_expr_forms` test (`compiler/crates/hy-emit/src/tests.rs:588`) must gain
  a plan-aware case (its no-plan path stays 2-index).
- **Performance:** every append into a clipped view allocates;
  `stack = append(stack[:len-1], x)` becomes O(n) per operation. The compat
  benchmark must gate the default.

## 4. Migration

- Land plan plumbing plus helpers behind `--clip-reslices`; run corpus and
  differential with it on; default after one cycle.
- Rewrite intentional aliasing views as `s[lo:hi:cap(s)]` /
  `s[lo:len(s):cap(s)]`; add parser support and a diagnostic fix-it.
- Replace `buf = buf[:0]` with `buf = buf[:0:cap(buf)]` or `buf.reset()`.
- Leave strings and `Unknown` bases untouched; document the residual.
- Add corpus/unit pins: bounded append isolation, open-ended isolation, array,
  string control (corpus 14), make-cap negative control, and a side-effecting
  bound called once.

## 5. Falsification criteria

- **P1 runtime:** build/run the six #1683 programs under
  `go run -race -count=100`; 0 races and `s[1]` unchanged. Negative control:
  `make([]T, 1, 4)` must still be caught by the checker, not by this proposal.
- **P2 string control:** corpus 14 emits `payload[a:b]`, builds, and runs
  byte-exact; no 3-index string.
- **P3 one evaluation:** `s[0:f()]` calls `f` once; an instrumented counter
  proves it.
- **P4 open-ended completeness:** `s := make([]int, 1, 4); t := s[:4];
  append(s[0:], 9)` leaves `t[1] == 0`. If `s[0:]` is left 2-index, `t[1] == 9`
  — fail.
- **P5 coverage census:** count `Expr::Slice` spans with `Unknown` bases across
  the corpus and examples; fail if any P0 repro or more than 5% of slice
  expressions fall through, or if corpus 14 is ever clipped.
- **P6 performance:** allocations/op and ns/op on append-heavy loops versus
  baseline; fail if the `buf[:0]` path loses more than the agreed budget or a
  clipped view allocates more than one fresh array per append.

## 6. Honest risk and bottom line

The fatal flaw is not the capacity idea; it is the missing type oracle and the
string/array distinction. A blind `s[lo:hi:hi]` rewrite breaks buildable Go on
corpus 14. The salvageable subset — checker-planned clipping of provably
slice/array views, helper-based single evaluation, explicit `max` for
intentional spare capacity — is implementable, kills the reproduced #1683
family, and leaves strings untouched. It is a strong soundness hardening, but
it is **not** the "delete spare-capacity analysis" revolution the issue
advertises: make-cap and foreign slices keep `loans.rs`/`freeze.rs` alive, and
`cap()`/buffer-reset semantics make the migration a breaking change. If the
maintainer wants the full "no ambient spare capacity" claim, the next step is
to also ban spare-capacity `make`/foreign slices or wrap slices in a `cap ==
len` type (W-001/W-003 territory); that is a larger design, not an emitter
patch.

**Recommendation:** accept the scoped version (M1) and falsify P1–P6 before
promising default-on. If the review only accepts the unconditional rule,
decline this entry: it cannot preserve corpus 14 and buildable Go.
