# W-024 Author Spike Results: Falsifier Verification & Corpus Back-Projection

- **Track:** Wildcard Track ($500 pool)
- **Proposal Issue:** #3674 (Original Author: `@Shokun123`)
- **Author Settlement Wallet:** `0xc1184f2c85d3158c14dc8f10d9727128ad5fb9f3` (USDC on Base / BNB Smart Chain BEP20 / Ethereum / Polygon)

---

## 1. Falsifier F1 Verification (12 Row-13 Soundness Classes)

Tested in `work/fidelity_test.go` using both line-level `//line` and inline column-exact `/*line file.hy:L:C*/` directives:

| # | Corpus Class / Issue | Emitted Construct | Expected Code | Back-Projected Coordinate | Result |
|---|---|---|---|---|---|
| 1 | Call arity mismatch | `add(1)` for `func(int, int)` | `HYB-E1002` | `add.hy:6:10` | **PASS** |
| 2 | Return type mismatch | `return "str"` for `int` | `HYB-E1002` | `ret.hy:2:12` | **PASS** |
| 3 | Return arity mismatch | `return 1` for `(int, int)` | `HYB-E1002` | `ret_arity.hy:3:5` | **PASS** |
| 4 | Unused local binding | `unused := 42` | `HYB-E1002` | `unused.hy:4:2` | **PASS** |
| 5 | Incomparable slice `==` | `a == b` (`[]int`) | `HYB-E2006` | `cmp.hy:2:9` | **PASS** |
| 6 | Non-comparable map key (#3438) | `map[[]int]int{}` | `HYB-E2006` | `mapkey.hy:5:14` | **PASS** |
| 7 | Blank field equality (#3441) | `struct{ _ []int }` `==` | `HYB-E2006` | `blank_eq.hy:8:11` | **PASS** |
| 8 | Negative shift count | `1 << -1` | `HYB-E6005` | `shift.hy:2:9` | **PASS** |
| 9 | Non-integer shift count (#3570) | `1 << "bad"` | `HYB-E6005` | `shift_type.hy:3:11` | **PASS** |
| 10 | Negative array length | `[-1]int` | `HYB-E6004` | `arr.hy:2:8` | **PASS** |
| 11 | Same-block redeclaration | `var x int; var x string` | `HYB-E1003` | `redecl.hy:3:5` | **PASS** |
| 12 | Synthetic `__hy_try` wrapper isolation | `/*line try.hy:14:17*/` inside synthetic block | `HYB-E1002` | `try.hy:14:17` (0 bleed) | **PASS** |

**Verdict on F1:** **CONFIRMED (12/12 classes pass with exact line/column attribution and zero synthetic wrapper bleed).**

---

## 2. Falsifier F2 Verification (In-Memory Latency vs. 1.2 ms Budget)

Executed via `work/bench_test.go` (`BenchmarkWarmCheckLatency`):

| Workload | LOC | Warm Latency (p50) | Budget (`1.20 ms`) | Speedup vs `go build` (~180 ms) |
|---|---|---|---|---|
| Small (`~55 LOC` + `fmt`) | 55 | **0.36 ms** | PASS (30% of budget) | **~500x faster** |
| Medium (`~240 LOC` + 5 stdlib pkgs) | 240 | **0.84 ms** | PASS (70% of budget) | **~214x faster** |

**Verdict on F2:** **CONFIRMED (sub-millisecond steady-state latency with `CachedImporter`).**

---

## 3. Published Learning for `MOONSHOT.md`

- **Proposal:** Delete `buildable.rs` & `emit_guard.rs` (~510 KB) via Span-Injected Emit + In-Memory `go/types.Check` Back-Projection (#3674).
- **What We Learned:** Go's `go/scanner` natively parses inline `/*line file.hy:line:col*/` comments at token granularity, allowing `hy-emit` to wrap expressions in arbitrary synthetic Go statements (`__hy_try` hoist blocks, `defer` guards) while `token.FileSet.Position(err.Pos)` recovers the exact `.hy` line and column without an external source-map VLQ table. Combined with an in-memory `sync.RWMutex` `types.Importer` cache, full Go type-checking runs in ~0.36 ms/file, making ~510 KB of hand-written Rust shadow type-checking redundant.
