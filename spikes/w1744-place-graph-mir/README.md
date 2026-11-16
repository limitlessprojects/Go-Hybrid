# Wildcard `#1744` Feasibility Spike — Staged Place-Graph Canonicalization (`w1744-place-graph-mir`)

- **Proposal**: Issue [`#1744`](https://github.com/limitlessprojects/Go-Hybrid/issues/1744) — *Capability-Carrying Desugared MIR (`hy-mir`): Place-Graph Canonicalization Before Tier-0 Ownership & Sendability Checking*
- **Incorporates Review & Pre-Registered Falsifiers**: PR [`#1934`](https://github.com/limitlessprojects/Go-Hybrid/pull/1934) (`research/wildcard-1744-place-graph-staged.md` by `@milkingndb-oss`)
- **Review Window**: Discussion [`#365`](https://github.com/limitlessprojects/Go-Hybrid/discussions/365) (*Wildcard review window — 2026-11-01*)
- **Production Impact of This Spike PR**: **Zero production files touched** (`spikes/w1744-place-graph-mir/` only; `compiler/` is read-only).

---

## 1. Purpose & Relationship to `w016-hy-mir` (`#695`) and PR `#1934`

1. **Distinct Layer from `W-016` (`#695`, `spikes/w016-hy-mir/`)**:
   - `W-016` (`#695`) targets **post-check / emitter lowering** (`hy-check -> hy-emit` typed IR for code generation).
   - `#1744` targets **intra-checker place canonicalization** *inside* `hy-check`: collapsing surface AST expression wrappers and lowering every lvalue/rvalue/temporary/loan/capture into a single canonical `CanonicalPlace` (`BindingId` + `BindingKind` + `Vec<Proj>` + `subexpr_reads`) **before** Tier-0 ownership, sendability, addressability, and loan-window rules run.

2. **Adopting PR `#1934`'s Staged Architecture & Retracting the `~350 LoC` Estimate**:
   - `@milkingndb-oss`'s analysis in PR `#1934` is right on both counts:
     - **Architectural thesis holds**: independent AST pattern matching across `walk.rs`, `freeze.rs`, `sendability.rs`, `addressable.rs`, `loans.rs`, and `closures.rs` is the common root cause of a recurring class of `ACCEPT -> REJECT` soundness bugs (`#1742`, `#1743`, `#1771`, `#1775`, `#1778`, `#1781`, `#1792`, `#1814`, `#1820`).
     - **`~350 LoC` single-pass estimate retracted**: `hy-check/src` has `1,336` `Expr::` sites across 21 files (`676` in the 6 core rule files), `258` defensive `unwrap_parens` call sites, and `160+` lines of closure/capture machinery. Even this standalone prototype is **`1,745` non-comment LoC** (`Stage 1 = 411`, `Stage 2 = 1,036`, `Stage 3 = 226`). We explicitly adopt `#1934`'s three-stage shippable plan (`wrappers -> places -> captures`, `~280 + ~1,150 + ~750 ≈ 2,180 LoC` production delta).

---

## 2. Three-Stage Architecture Implemented in `src/lib.rs`

| Stage | Module Entrypoint | Core Invariant | Spike LoC | Production Delta Budget (`#1934`) |
| --- | --- | --- | ---: | ---: |
| **Stage 1** (`wrappers`) | `normalize_file(&File) -> (File, Stage1Stats)` | Recursive erasure of `Expr::Paren` and normalization of `Expr::Unary { op: UnOp::Addr }` -> `Expr::AddrOf` across all declarations, types, statements, places, and expressions while preserving inner `Span`s. | `411` | `~280 LoC` |
| **Stage 2** (`places`) | `PlaceGraphChecker::lower_place` & `check_file_place_graph(..., ClosurePolicy::Stage2Conservative)` | Every binding and materialized rvalue temporary has a canonical `BindingId` and `BindingKind` (`UserLocal \| ParamOrReceiver \| SlotLoan \| BorrowView \| Temp(CompositeLit \| RvalueResult)`). Projections (`Field \| Index \| Slice`) retain `subexpr_reads` and map-value containment flags (`at_map_index`, `has_post_map_value_proj`). | `1,036` | `~1,150 LoC` |
| **Stage 3** (`captures`) | `PlaceGraphChecker::summarize_closure` & `ClosurePolicy::Stage3CapturePlaces` | Closures lower free outer bindings into `Vec<CapturePlace>` (`Read \| Mutate \| Move`). Calling a closure at step $t$ extends captured loan `last_use` windows to $t$ (`#1814`) and records container mutations (`#1792`). | `226` | `~750 LoC` |

---

## 3. Reproduce All 5 Pre-Registered Falsifier Tests (`F1..F5`)

From the repository root (with pinned Rust toolchain env vars):

```bash
HYB_PINNED_RUST=1.98.0 HYB_PINNED_GO=1.27.0 \
  cargo test --manifest-path spikes/w1744-place-graph-mir/Cargo.toml -- --nocapture
```

See [`RESULTS.md`](./RESULTS.md) for the exact recorded outputs against `main @ 15e17e3afb3e`.
