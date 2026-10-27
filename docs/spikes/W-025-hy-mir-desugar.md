# W-025 (#695) Spike: Emit-Driven Verification via Explicit Desugared MIR (`hy-mir`)

## 1. Summary & Ledger Reference
- **Issue:** `#695` (`[wildcard] Eliminate the Checker/Emitter Semantic Split: Emit-Driven Verification via Explicit Desugared MIR (hy-mir)`)
- **Ledger ID:** `W-025` (`$500` USD Wildcard Track, `0` open competing PRs)
- **Architecture:** Lowers surface AST into explicit `hy-mir` instructions (`AllocMut`, `AddrOf`, `SlotLoanCopy`, `SlotLoanStoreBack`, `TaskSpawn`) *before* `hy-check` runs, turning `hy-emit` into a stateless 1:1 printer (`0` shadow type tables: `fn_shapes`, `var_ty`, `var_struct`, `pointer_vars`, `loan_frames` deleted).

## 2. M1 Falsifiable Invariant & Impedance Bug Matrix (`#188`, `#546`, Auto-Addressing)
| Impedance Class | Legacy Dual-Pass Failure | `hy-mir` Desugared Lowering + Check Verdict |
|---|---|---|
| **`#188` Composite `const s = stats{}`** | `hy-emit` lowers to `var s`, allowing pointer mutation | Lowered to `AllocConst`; `AddrOf(s)` rejected (`HYB-E3004` / `T22`) |
| **`#546` Branch map-slot loan (`&m[k]`)** | Implicit scope-exit writeback races with `task` move | Explicit `SlotLoanStoreBack` after `TaskSpawn(move m)` rejected (`HYB-E3104`) |
| **Pointer-param auto-addressing `worker(s)`** | `hy-emit` inserts `&s` unseen by `hy-check` | Explicit `AddrOf(s)` in `hy-mir` caught after move (`HYB-E3001`) or in loop (`HYB-E3003`) |
| **1:1 `hy-mir` <-> Go Emit Bijection** | Emitter invents unverified `&` / `var` / store-backs | `verify_bijection()` enforces 1-for-1 match between MIR and Go output |
