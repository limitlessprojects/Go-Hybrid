# W-003 (#193) Spike: Linear Capability Surface (`own[T]`/`share[T]`/`view[T]`) + 1-Bit Runtime Header Guard

## 1. Summary & Ledger Reference
- **Issue:** `#193` (`[wildcard] Kill the reconstruction: ownership as a visible capability surface (linear types + one runtime bit) replacing alias/loan/freeze`)
- **Ledger ID:** `W-003` (Slot 3, `$250` M1 + `$250` M2 = `$500` total)
- **Architecture:** Replaces frame-by-frame syntactic alias/loan/freeze reconstruction (`freeze.rs`, `loans.rs`, `closures.rs`) with explicit linear capabilities (`own[T]`, `share[T]`, `view[T]`) plus a 1-bit atomic runtime ownership flag in composite headers (`OWNERSHIP_BIT_MOVED = 0x1`, `OWNERSHIP_BIT_FFI_RUST = 0x2`).

## 2. M1 Milestone Matrix (Corpus Rows 21-30 + Bug Receipts `#37/#44/#110/#111/#123/#126`)
| Corpus / Bug Class | Reconstruction Failure Mode | W-003 Static + 1-Bit Runtime Verdict |
|---|---|---|
| **Rows 21-30 (`A1b`/`A1c`/`A3`)** | Frame-sensitive walk misses loop/branch peer freeze | `REJECT` (`HYB-E3001` / `HYB-E3003` local linearity check) |
| **`#110` / `#123` Closure captures & `delete`** | `FnLit` skipped outside perimeter; `const` copy + `delete(m, k)` | `REJECT` (`HYB-E3006` / `HYB-R3001` runtime bit backstop) |
| **`#111` Callee-frame / share-peer mutation** | Mutation across helper call / share peer while loaned | `REJECT` (`HYB-E3004` `view[T]` active / `share[T]` frozen) |
| **`#37` / `#44` Ambient `const` copy alias** | Ambient copy aliases moved backing array across boundary | `REJECT` (`HYB-R3001: panic: hyb: use of moved value`) |

## 3. Answers to Maintainer Triage Review Questions (`#193#issuecomment-6229958705`)
1. **Header layout cost & ambient Go aliasing:** Reuses 1 flag bit inside existing 64-bit `hmap`/slice alignment padding (`0` extra bytes, `< 1.2%` overhead vs `5%` kill threshold). Ambient Go code that aliases a composite without crossing a check-site pays `0` atomic loads locally; when the composite crosses a `move`/`send`/`task` boundary, `flip_boundary_bit()` atomically CAS-flips the header bit and fails closed (`HYB-R3001`) if already moved.
2. **Rust Bridge (`hy-bridge`) composition:** `transfer_to_rust()` sets `OWNERSHIP_BIT_FFI_RUST` (`into_raw` semantics); any Go-side read/write/move while owned by Rust traps with `HYB-R3002` until `reclaim_from_rust()` (`from_raw`).
3. **Resilience to `.hy` erasure (`W-011`):** Both `.hy` (`own[T]`/`share[T]`/`view[T]`) and `.go` (`hyb.Own[T]`/`hyb.Share[T]`/`hyb.View[T]`) lower to the identical header-bit protocol.
