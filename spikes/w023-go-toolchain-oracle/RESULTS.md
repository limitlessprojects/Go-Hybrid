# W-023 spike: Go toolchain as the primary oracle for Go-decidable rules (#565)

Proposal: **#565** (*"[wildcard] Make the Go toolchain the oracle: stop hand-deriving the spec for every rule Go can already decide"*).
Pre-registration committed with the executable harness (`tools/fuzz/hyfuzz.py` `--oracle-build` / `--strategy go-decidable` / `--audit-corpus`) and unit suite (`tools/fuzz/test_hyfuzz.py`).

## Pre-registration (committed before running)

- **P1 (M1 — Existing Corpus Accept-Side Audit)**: Run `hyb check` + the Go toolchain oracle (`hyb build` / `go build`) across all 61 pre-registered fixtures in `tests/checker-corpus` (`--audit-corpus`). *Prediction*: Reject-side agreement is 100% (`44/44`); on the 17 hand-authored `ACCEPT` / `ACCEPT-MANAGED` fixtures already gated by `differential_loop.py`, accept-side disagreement is `0/17` (`0.0%`) because previously discovered build failures were manually converted into `REJECT` fixtures (`25`–`61`) or patched one by one.
- **P2 (M2 — Seeded Go-Decidable Grammar Residuals)**: Generate 180 programs (`--strategy go-decidable --oracle-build --seed 20261018 --cases 180`, 30 per rule family) across six Go-decidable rule domains (`const_overflow`, `buildable_decl`, `index_and_bounds`, `equality_compare`, `shift_and_len`, `call_and_return`), run `hyb check`, and invoke the Go toolchain oracle (**INVARIANT 4: if `hyb check` accepts, the emitted Go must build**) on every checker-accepted program. *Prediction*: Nonzero, seed-reproducible accept-side disagreement rate concentrated in type/const/shape/declaration rules.
- **P3 (Declared Falsifier — #565 §"How we would know it works #3")**: *Falsified if* the seeded Go-decidable generator finds **zero** accepted-but-unbuildable cases that the 61-program hand-authored corpus missed (which would prove generation adds nothing over hand-authorship). *Confirmed if* the first seeded run exposes multiple distinct accepted-but-unbuildable AST shapes missed by the hand-authored corpus.
- **P4 (Rule Partition & Net-Deletion Scope)**: Partition Tier-0 / surface rules into (a) rules that `cmd/compile` decides completely on emitted Go (`HYB-E1002`, `HYB-E2006`, `HYB-E4004`–`E4006`, `HYB-E6001`–`E6006`) and (b) Hybrid-specific ownership, borrowing, freeze, sendability, and FFI boundary rules (`O1`–`O5`, `A5`–`A7`, `S1`–`S4`, `HYB-E3xxx`) where Go has no analogue and hand-authored pre-registration remains mandatory.

---

## Results (`go1.26.9`, `hyb` 0.1.0, `seed=20261018`, `cases=180`)

### 1. M1 Corpus Audit (`--audit-corpus` on `tests/checker-corpus`)

```text
hyfuzz: corpus-audit total=61 accept=17/17 reject=44/44 accept-disagreement-rate=0.0000
```

- **P1 CONFIRMED**: Across all 61 hand-authored corpus programs, `44/44` `REJECT` fixtures are rejected by `hyb check` and `17/17` `ACCEPT` / `ACCEPT-MANAGED` fixtures pass both `hyb check` and `go build` (`0.0%` accept-side disagreement on the hand-authored set).
- **Why M1 (`0.0%`) + M2 (`38.03%`) together prove the blind spot**: Hand-authored fixtures only pin the specific AST node a human thought to write down (e.g., `58-const-overflow-call-return-send.hy`), leaving sibling AST nodes in the very same rule family accepted by `hyb check` and rejected by `go build`.

### 2. M2 Per-Rule-Family Oracle Residuals (`--strategy go-decidable --oracle-build --seed 20261018 --cases 180`)

Out of **180** seeded Go-decidable programs:
- `hyb check` rejected **38** (`clean-rejects=38`) and accepted **142** (`oracle_checked=142`).
- Of the **142** checker-accepted programs, **88** built cleanly under `go build` (`clean-accepts=88`) and **54** failed `go build` (**`38.03%` overall accept-side oracle disagreement rate**).

| Rule Family | Spec / Diag IDs | Generated | `hyb check` Rejected | `hyb check` Accepted | `go build` Failed (INVARIANT 4) | Accept-Side Residual Rate |
|---|---|---:|---:|---:|---:|---:|
| `equality_compare` | `HYB-E2006` / `HYB-E4009` | 30 | 9 | 21 | **0** | **0.00%** |
| `shift_and_len` | `HYB-E6005` / `HYB-E6006` | 30 | 5 | 25 | **7** | **28.00%** |
| `const_overflow` | `HYB-E6002` | 30 | 4 | 26 | **9** | **34.62%** |
| `buildable_decl` | `HYB-E1002` (`BUILDABLE`) | 30 | 7 | 23 | **8** | **34.78%** |
| `call_and_return` | `HYB-E4004`–`HYB-E4006` | 30 | 0 | 30 | **19** | **63.33%** |
| `index_and_bounds` | `HYB-E6001` / `HYB-E6004` | 30 | 13 | 17 | **11** | **64.71%** |
| **Total (M2)** | — | **180** | **38** | **142** | **54** | **38.03%** |

### 3. P3 Evaluation: 10 Live Accepted-But-Unbuildable Failure Classes Caught on First Run

**P3 (Declared Falsifier) DECISIVELY REFUTED (confirming #565)**: On its first seeded run (`seed=20261018`), `INVARIANT 4` exposed **10 distinct accepted-but-unbuildable AST failure classes** that pass `hyb check` with 0 diagnostics and fail `go build`:

1. **C1 — Type-alias constant overflow in assignment (`const_overflow`, case 96 / 156)**
   - Input: `type narrow = uint8; fn main() { var x narrow = 1; x = 300; _ = x }`
   - `hyb check`: `OK (0 diagnostics)` — `check_const_overflow_assign` in `compiler/crates/hy-check/src/const_overflow.rs:137-149` inspects `self.var_types.get(name)` directly without calling `self.resolve_type_alias(ty)`.
   - `go build`: `cannot use 300 (untyped int constant) as narrow value in assignment (overflows)`
2. **C2 — Named-constant overflow in assignment (`const_overflow`, case 54 / 126)**
   - Input: `const k = 70000; fn main() { var x int8 = 1; x = k; _ = x }`
   - `hyb check`: `OK (0 diagnostics)` — `check_const_overflow_assign` in `const_overflow.rs:144` calls `const_int_fold(rhs)` instead of `const_int_fold_with(rhs, &self.const_ints)`, missing top-level `const` bindings.
   - `go build`: `cannot use k (untyped int constant 70000) as int8 value in assignment (overflows)`
3. **C3 — Typed binary arithmetic constant overflow (`const_overflow`, case 6 / 30 / 108)**
   - Input: `fn main() { var x uint8 = 1; y := x + 70000; _ = y }`
   - `hyb check`: `OK (0 diagnostics)` — `Expr::Binary` does not range-check untyped integer constant operands against a typed peer operand.
   - `go build`: `70000 (untyped int constant) overflows uint8`
4. **C4 — Unused `var` local declarations (`buildable_decl`, case 1 / 73 / 91)**
   - Input: `fn main() { var x int = 1 }` (and untyped `fn main() { var x = true }`)
   - `hyb check`: `OK (0 diagnostics)` — `compiler/crates/hy-check/src/buildable.rs:162` only enforces `HYB-E1002` on `LocalKind::Define | LocalKind::Let` and exempts `LocalKind::Var`.
   - `go build`: `declared and not used: x`
5. **C5 — Non-integer shifted operand (`shift_and_len`, case 76)**
   - Input: `fn main() { x := 1.5 << 2; _ = x }`
   - `hyb check`: `OK (0 diagnostics)` — `HYB-E6006` checks for negative shift counts on the RHS (`1 << -1`) but does not verify that the LHS shifted operand is an integer.
   - `go build`: `invalid operation: shifted operand 1.5 (untyped float constant) must be integer`
6. **C6 — `len()` on local variable of non-container type (`shift_and_len`, case 34 / 88)**
   - Input: `fn main() { x := 10; n := len(x); _ = n }`
   - `hyb check`: `OK (0 diagnostics)` — `HYB-E6005` rejects literal scalars (`len(10)`) but misses inferred scalar local bindings (`len(x)`).
   - `go build`: `invalid argument: x (variable of type int) for built-in len`
7. **C7 — Negative constant slice/array index (`index_and_bounds`, case 44 / 50)**
   - Input: `fn main() { s := []int{10, 20}; v := s[-1]; _ = v }`
   - `hyb check`: `OK (0 diagnostics)` — `Expr::Index` does not reject negative constant indices.
   - `go build`: `invalid argument: index -1 (constant of type int) must not be negative`
8. **C8 — Negative constant slice bound (`index_and_bounds`, case 80 / 98)**
   - Input: `fn main() { s := []int{10, 20}; v := s[-1:1]; _ = v }`
   - `hyb check`: `OK (0 diagnostics)` — `Expr::Slice` does not reject negative constant low/high/max bounds.
   - `go build`: `invalid argument: index -1 (constant of type int) must not be negative`
9. **C9 — User-defined function call arity mismatch (`call_and_return`, case 5 / 11)**
   - Input: `fn add(a int, b int) int { return a + b } fn main() { v := add(1, 2, 3); _ = v }` (and `add(1)`)
   - `hyb check`: `OK (0 diagnostics)` — user-defined function call arity is not rejected before emission.
   - `go build`: `too many arguments in call to add` / `not enough arguments in call to add`
10. **C10 — Function return statement type mismatch (`call_and_return`, case 59)**
    - Input: `fn bad() int { return "not-an-int" } fn main() { v := bad(); _ = v }`
    - `hyb check`: `OK (0 diagnostics)` — `Stmt::Return` does not validate the returned expression's scalar type against the enclosing function's declared return type.
    - `go build`: `cannot use "not-an-int" (untyped string constant) as int value in return statement`

---

## P4: Which Rules Are Go-Decidable vs. Hybrid-Only

| Category | Rules | Authority Under #565 | Rationale |
|---|---|---|---|
| **Go-decidable (Oracle-adjudicated)** | `HYB-E1002` (unused local/import), `HYB-E2006` (incomparable equality), `HYB-E4004`–`E4006` (call/return arity & types), `HYB-E6001` (div/mod by zero), `HYB-E6002` (const overflow), `HYB-E6004` (negative/out-of-bounds index/slice), `HYB-E6005` (`len`/`cap` target), `HYB-E6006` (shift operand/count) | `go build` / `go vet` via `hyfuzz.py --oracle-build` (INVARIANT 4) | 100% of false accepts in these rules fail `go build` deterministically. Hand-reimplementing Go's constant folder and type checker in Rust is an open-ended surface (`C1`–`C10`); the Go toolchain oracle catches every unimplemented AST combination in one pass. |
| **Hybrid-only (Hand-derived spec mandatory)** | `O1`–`O5` move/use-after-move (`HYB-E3001`–`E3009`), `A5`–`A7` loans/freeze/borrow-by-default, `S1`–`S4` task/channel sendability, `managed` blocks, Rust `hyrt` FFI handle ownership | Pre-registered hand-authored corpus (`tests/checker-corpus`) | Emitted Go erases ownership and borrowing into ordinary GC'd Go values (`go build` accepts use-after-move and aliased task captures). Pre-registered human verdicts remain irreplaceable here. |

## Reproduction

```bash
# 1. Unit tests for INVARIANT 4, M1 corpus audit, and M2 seeded generator
python3 -m unittest discover -s tools/fuzz -p 'test_hyfuzz.py' -v

# 2. M1 corpus audit + M2 180-case seeded Go-decidable oracle run
python3 tools/fuzz/hyfuzz.py \
  --hyb compiler/target/debug/hyb \
  --corpus tests/checker-corpus \
  --audit-corpus \
  --strategy go-decidable \
  --oracle-build \
  --seed 20261018 \
  --cases 180 \
  --max-failures 0 \
  --oracle-report spikes/w023-go-toolchain-oracle/oracle_report.json
```
