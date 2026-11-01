# Pre-Registered Falsifier Results (`F1..F5`) — Wildcard `#1744`

Recorded against unpatched `main` (`15e17e3afb3ebc12ae5bfa01c575ea4db5bb82f6`) via:

```bash
cargo test --manifest-path spikes/w1744-place-graph-mir/Cargo.toml -- --nocapture
```

---

## Summary Matrix (Answering PR `#1934` §4.1–§4.5)

| Falsifier | PR `#1934` Gate / Question | Empirical Result on `main @ 15e17e3` | Verdict |
| --- | --- | --- | --- |
| **`F1` — Stage 1 Corpus Parity** (`§4.1`) | Run Stage-1 normalization before unmodified `hy_check::check_file` across all `tests/checker-corpus/**/*.hy`; require **0 verdict flips** and **0 diagnostic code changes**. | **`129 / 129` corpus files identical** (`9` `Expr::Paren` wrappers erased; `0` verdict flips; `0` diagnostic code vector diffs). | **PASS** |
| **`F2` — Stage 1 Wrapper Falsifier** (`§4.2`) | Prove Stage-1 normalization closes live wrapper bypass `#1743` (`task worker((some(&s.a)), done)` and `ch <- (some(&s.a))`) **without** editing `sendability.rs`. | Unpatched `hy_check::check_file` falsely returns `Accept` on both; `check_file(&normalize_file(&ast).0)` returns **`Reject` (`HYB-E3001` / `HYB-E3002`)** with **0 checker edits**. | **PASS** |
| **`F3` — Stage 2 Place-Graph Unification** (`§4.3`) | Prove a canonical `PlaceId` + `BindingKind` graph kills multiple independent AST bug families with uniform place predicates while passing positive controls. | **5 live `main` false-accept bug classes** (`#1742`, `#1775`, `#1778`, `#1814`, `#1820`) + `TempKind::RvalueResult` projection (`HYB-E2012`) all flip `Accept -> Reject`; positive control (`&box{x: 1}` + valid `ch <- move h.doc`) passes with `0` diagnostics. | **PASS** |
| **`F4` — Stage 2 Closure Cost & Stage 3 Captures** (`§4.4`) | Measure Stage-2 conservative closure rejection rate across all `Accept` corpus files (gate: **`<= 5.0%`** so Stage 2 can ship independently), and prove Stage 3 `CapturePlace` catches `#1814` & `#1792`. | Conservative Stage-2 rule rejects **`1 / 41` `Accept` corpus files (`2.44% <= 5.0%`)**. Stage-3 `CapturePlace` rejects live `main` false-accepts **`#1814` (`HYB-E3101`)** and **`#1792` (`HYB-E3102`)**. | **PASS** |
| **`F5` — Coupling Census & LoC Honesty** (`§4.5`) | Automated census of `compiler/crates/hy-check/src/*.rs` (`Expr::`, `Expr::Paren`, `unwrap_parens`, `FnLit`/`closure`) and non-comment LoC audit of the staged implementation. | `21` files scanned: **`1,336` `Expr::` sites**, **`36` `Expr::Paren` arms**, **`258` `unwrap_parens` calls**, **`160` `FnLit`/closure lines**; standalone spike is **`1,745` non-comment LoC** (`411 + 1,036 + 226 + 72`), confirming `#1934`'s staged budget over `#1744`'s original `~350 LoC` estimate. | **PASS** |

---

## Real Filed Soundness Bugs Unified by Stages 1–3

| Issue | Soundness Failure on AST-Pattern Checker | Canonical Place-Graph Invariant That Eliminates It | Stage |
| --- | --- | --- | :---: |
| [`#1743`](https://github.com/limitlessprojects/Go-Hybrid/issues/1743) | `(some(&s.a))` bypasses `classify_value` (`HYB-E3001`/`E3002`) | `Expr::Paren` erased once in `normalize_file` before checking | **Stage 1** |
| [`#1742`](https://github.com/limitlessprojects/Go-Hybrid/issues/1742) | `managed { task w(move h.m) }` skips O5.4 (`HYB-E3004`) and post-move freeze (`HYB-E3003`) | `check_move_transfer` inspects `CanonicalPlace` (`BindingKind::UserLocal` + `terminal_is_own_handle`) and freezes `share_component(place.root)` uniformly | **Stage 2** |
| [`#1775`](https://github.com/limitlessprojects/Go-Hybrid/issues/1775) | `&n.arr[0]`, `&n2[0]`, `&n.arr[0:2]` on map-slot loan `n := &m["a"]` miss `HYB-E3104` | `Expr::AddrOf` checks `self.bindings[place.root].kind == BindingKind::SlotLoan` for *any* projection depth | **Stage 2** |
| [`#1778`](https://github.com/limitlessprojects/Go-Hybrid/issues/1778) | `ch1 <- move h.items[idx["k"]].doc` drops reads inside `Index` projections of moved field chains (`HYB-E3003`) | `lower_place` retains `subexpr_reads` on every `Proj::Index` / `Proj::Slice` hop | **Stage 2** |
| [`#1814`](https://github.com/limitlessprojects/Go-Hybrid/issues/1814) (chain) | `c := move holders[p.idx].conn` drops `p.idx` read, failing to extend slot loan `p` past `m["b"] = ...` (`HYB-E3102`) | `subexpr_reads` on `CanonicalPlace` records `p` at the move step, extending `p`'s loan window automatically | **Stage 2** |
| [`#1820`](https://github.com/limitlessprojects/Go-Hybrid/issues/1820) | `(&m["a"].bal) == nil` and `&arr[0]["a"].bal` bypass statement-shape `HYB-E2010` checks | `CanonicalPlace::has_post_map_value_proj` fires `HYB-E2010` on *every* `Expr::AddrOf` anywhere in the expression tree | **Stage 2** |
| [`#1814`](https://github.com/limitlessprojects/Go-Hybrid/issues/1814) (closure) | Closure capturing `v := doc.array("k")?` called after `doc.set("k", "x")` misses `HYB-E3101` | `CapturePlace { outer: v, mode: Read }` records a read of `v` at `read_v()`'s call step | **Stage 3** |
| [`#1792`](https://github.com/limitlessprojects/Go-Hybrid/issues/1792) | Closure calling helper method `s.put(...)` during `entry := &s.m["a"]` misses `HYB-E3102` | `CapturePlace { outer: s, mode: Mutate }` records a mutation of `s` at `bump()`'s call step | **Stage 3** |
| [`#1771`](https://github.com/limitlessprojects/Go-Hybrid/issues/1771) / [`#1781`](https://github.com/limitlessprojects/Go-Hybrid/issues/1781) | `defer` post-transfer use of moved binding & callee return-shape alias drift | `Defer` lowers to function-exit `CapturePlace` reads; typed `CanonicalPlace` resolves aliases once | **Stage 3** |

---

## Raw Test Output

```text
running 5 tests
test f2_stage1_wrapper_falsifier_closes_live_issue_1743_without_checker_edits ... ok
test f3_stage2_place_graph_eliminates_five_bug_classes_uniformly ... ok
[F5 CENSUS] hy-check/src (21 files): Expr:: sites = 1336, Expr::Paren arms = 36, unwrap_parens calls = 258, Closure/FnLit sites = 160; spike non-comment LoC = 1745 (Stage1=411, Stage2=1036, Stage3=226)
test f5_codebase_coupling_census_and_loc_budget_honesty ... ok
[F4 MEASUREMENT] Stage-2 conservative closure rule rejects 1/41 Accept corpus files (2.44%)
test f4_stage3_closure_conservatism_cost_on_corpus_and_capture_soundness_1792_1814 ... ok
[F1 PASS] 129/129 corpus files identical; erased 9 parens, normalized 0 Unary::Addr
test f1_stage1_corpus_parity_across_all_129_files ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
```
