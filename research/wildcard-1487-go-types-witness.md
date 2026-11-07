# Wildcard #1487 — a `go/types` witness behind the shape oracle, and exactly what it cannot replace

**Issue:** [#1487](https://github.com/limitlessprojects/Go-Hybrid/issues/1487) — *"Kill hy-check's hand-rolled Go typechecker: emit-first go/types witness-and-shape oracle"*

**Verdict up front:** the hand-rolled checker is larger than the proposal assumes and does two jobs that `go/types` cannot do: it is not only inferring Go types, it is *deciding* the Hybrid-only rulings (shapes, sendability, loans). A `go/types` witness can replace the inference half and remove a whole class of drift bugs. It cannot replace `shape.rs`, `sendability.rs`, or `loans.rs`, and a plan that claims otherwise will strand the corpus. What follows measures the surface, names the divergences with file:line, and pre-registers six tests that falsify the salvageable version.

## 1. The surface, measured

| Component | Size | What it actually is |
|---|---|---|
| `hy-check` | **37,997 lines / 21 files** | the checker |
| `buildable.rs` | 4,716 lines, 15 top-level fns | expression/statement admission |
| `shape.rs` | 1,520 lines | *"the lightweight TypeShape inference (**NOT a type system**)"* — its own header, line 1 |
| `loans.rs` | 8,421 lines | borrow/loan discipline (no Go analogue) |
| `sendability.rs` | 3,136 lines | A1 ruling (no Go analogue) |
| `emit_guard.rs` | 2,534 lines | emitter-side preconditions |
| `const_overflow.rs` | 1,499 lines | conversion/constant semantics |
| AST expression variants | **50** | `hy-syntax/src/ast.rs` |
| Builtin names with checker rules | **46** | `hy-check/src/builtins.rs` |
| Checker corpus | **129 files** | `tests/checker-corpus/` |
| `go/types` references in `hy-check` | **0** | `git grep -c 'go/types'` |

Zero references is the honest starting point: nothing in the checker consults the Go type checker, and `shape.rs` says so itself — everything unresolvable degrades to `Unknown`, which is *fail-closed at spawn/send boundaries* (`HYB-E3005`, O4.9) but legal inside a function. That design is deliberate, and it is why the checker can accept programs whose Go types are not yet known. A `go/types`-only checker would have to reject them.

The drift is already visible as filed bugs, which is the strongest argument for the direction:

- `#1803` — `complex64`/`complex128` conversions and `complex`/`real`/`imag` builtins omitted from constant-fold handling.
- `#1805` — void calls accepted in value positions; multi-value arity mismatches missed.
- `#1765` — builtin operand omissions: `real(string/nil)`, `imag(bool/nil)`.
- `#1775`, `#1778`, `#1822` — three separate admission holes where the hand-rolled model's local reasoning (address-of, move flags, method resolution by name) diverges from Go's rules.

Five independent bug classes in one subsystem is not a maintenance smell; it is the predictable cost of re-deriving Go's typing rules by hand. That is the part worth deleting.

## 2. What a witness buys, precisely

Two call sites consume the current inference:

- `hy-check/src/buildable.rs:1818` — `kind_of_expr`, resolves locals, calls, fields, literals.
- `hy-emit/src/expr.rs:567` — `type_of_expr`, resolves **only the shape table**, so emission cannot discriminate by type at all. This asymmetry (documented in the #1688 analysis) is why emission needs the checker's plan handed over rather than re-deriving types.

A witness does not have to replace `kind_of_expr`. It has to *arbitrate* it. Proposed shape of the change:

1. **Witness generation.** For a file under check, emit a synthetic Go file that 1:1 mirrors the Hybrid declarations and expression positions the checker needs typed, with each probe point assigned to a named blank (`var _ = probe_37`). Run `go/types.Config.Check` once per package, in-process.
2. **Witness record.** A `TypeWitness` maps `(file, byte-span) → go/types.Type`, serialized to a stable digest so the checker and emitter see the same oracle (this is the piece `type_of_expr` lacks today).
3. **Arbitration.** `kind_of_expr` keeps its fast path; where it returns `Unknown` **and** the witness has a concrete type, the witness wins and the shape is derived from it. Where they disagree, the checker fails closed with a new diagnostic carrying both types — that disagreement is a bug report, not a heuristic.
4. **Hybrid-only rulings stay.** `shape.rs` keeps the `TypeShape` lattice because sendability needs *provenance* (`move`-ness, capability), which `go/types` has no notion of. `sendability.rs` and `loans.rs` are untouched.

This is the "witness-and-shape" reading of the issue: the witness supplies Go truth, the shape lattice supplies Hybrid truth, and neither pretends to be the other.

## 3. Pre-registered tests (before any code)

1. **Corpus parity.** All 129 files in `tests/checker-corpus/` produce byte-identical diagnostics with the witness enabled but arbitration disabled. Any diff falsifies the "witness is inert when unused" claim.
2. **Unknown recovery rate.** On the corpus, count expression positions where `kind_of_expr` returns `Unknown` but the witness is concrete. Pre-registered prediction: **≥ 15% of positions**, concentrated in `option` element types and closure returns. Below 5% and the witness does not pay for itself.
3. **Divergence harvest.** Enable arbitration in report-only mode and collect every `kind_of_expr` vs. witness disagreement. Prediction: **≥ 10** disagreements, at least one matching the signature of `#1805` or `#1822`. Zero disagreements means our model is right and the issue is moot — worth knowing either way.
4. **Fail-closed preservation.** Craft three programs where the witness knows a type that the shape lattice cannot prove at a spawn boundary. All three must still be rejected with ``HYB-E3005``. A witness that *widens* acceptance at boundaries is a soundness regression.
5. **Emit/checker agreement.** For every `Expr::Slice` and composite literal in the corpus, the plan emitted by `hyb` and the witness type must agree on element type or the build fails. This is the guard that stops `type_of_expr`'s blindness from reappearing.
6. **Cost.** Witness generation + `go/types` on the full corpus must stay under **2× current check wall time** and under 200 MB RSS. If it does not, the design is a performance regression regardless of correctness.

## 4. What does not survive

- **The title.** "Kill the hand-rolled typechecker" is not achievable: ~13,500 of the 38,000 lines (`loans.rs`, `sendability.rs`, big parts of `shape.rs`) implement rulings that have no Go analogue. They stay.
- **Single-source typing.** Hybrid admits programs whose Go types depend on shapes the checker has not proven yet. A design that makes `go/types` authoritative will reject them; a design that keeps shapes authoritative keeps the drift. Arbitration — not replacement — is the only version that survives.
- **Generics and method sets.** The witness inherits Go's rules verbatim; where Hybrid's surface syntax cannot be mirrored 1:1 (capability-annotated types, `option` erasure), the witness must be generated post-erasure, which means the probe file is *not* a syntactic copy of the source. That translation layer is the real cost and the place a future bug will live.

## 5. Cost

Roughly: 3–4 weeks of review bandwidth, one new crate (`hy-witness`), one serialized artifact (`TypeWitness` digest) threaded from `hy-check` into `hy-emit` — the same handoff that `hyb` already lacks (`compiler/crates/hyb/src/main.rs:512`, `:582`). The win is structural: five classes of filed soundness bugs (`#1765`, `#1803`, `#1805`, `#1822`, `#1775`) become witness discrepancies instead of silent admissions, and the emitter finally gets the type oracle it has never had.
