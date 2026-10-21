# Spike 3 (`W-003` / #193) - Capability Types + One Runtime Ownership Bit (Feasibility Prototype)

- **Ledger ID:** `W-003` (Slot 3, Family A, 2026-11-01 review window in [#365](https://github.com/limitlessprojects/Go-Hybrid/discussions/365))
- **Source Proposal:** [#193](https://github.com/limitlessprojects/Go-Hybrid/issues/193)
- **Cross-Reference Family:** `W-001` (#304, PR #336), `W-017` (#200, PR #370), `W-014` (#239, PR #381)
- **Scope note:** this is a bounded, zero-dependency Go prototype of the *runtime* half of W-003 - the one-bit boundary check - not a compiler patch. It is pending maintainer verification, like the other family spikes.

---

## 1. What this spike demonstrates

W-003 claims: with `own[T]`/`share[T]`/`view[T]` in the type layer and **one ownership bit per composite** in the runtime layer, every accept-then-race seam the checker reconstruction ships late (#110/#111/#123/#126; the PR #214..#218 backedge family; the #37/#44 const-write family) **fails closed at the boundary** instead of racing. This spike realizes exactly that runtime layer in plain Go - no dependencies, no compiler changes - and measures what the bit costs.

## 2. Pre-registered falsifiers and outcomes

Three falsifiers were pre-registered in the proposal's M1 sketch before any code ran. The verdict of all three, against the running prototype:

| # | Falsifier | Claim under test | Outcome |
|---|-----------|------------------|---------|
| P1 | `own-use-after-move` at a boundary runs instead of failing closed | second `Own.Move()` rejects with `hyb: use of moved value ... (fail-closed)` | **NOT falsified** (T1 PASS) |
| P2 | a share *peer* crosses the boundary after its peer already moved | exactly one crossing per peer set; later peers reject (the PR #214 seam) | **NOT falsified** (T2 PASS) |
| P3 | boundary check allocates or costs > 50 ns/op | one CAS on a preallocated bit: **0 B/op, 0 allocs/op**, 13.03 ± 0.15 ns/op including the capability re-mint | **NOT falsified** (bench below) |

A fourth seam was declared out of the runtime spike's reach before running: **view-storage (storing a `View[T]` inside a struct field)** is the *type-level* half of the proposal and is not enforced by this prototype (limitation L2 below). Declared in writing per the repo's "honesty is the deliverable" operator agreement, not discovered after the fact.

## 3. Measured results (`go test -bench . -benchmem -count=3`, AMD64, Go 1.27.0)

```
BenchmarkBoundaryMove-8   	13.03 ns/op	       0 B/op	       0 allocs/op
                            12.95 ns/op	       0 B/op	       0 allocs/op
                            13.10 ns/op	       0 B/op	       0 allocs/op
BenchmarkBaselineCopy-8    	 0.48 ns/op	       0 B/op	       0 allocs/op
                             1.02 ns/op	       0 B/op	       0 allocs/op
                             1.17 ns/op	       0 B/op	       0 allocs/op
```

- **Boundary crossing (own[T], full lifecycle per iteration):** ~13.03 ns/op, **0 allocs**. The whole lifecycle is one store + one CAS on a bit preallocated at the mint site.
- **Baseline (plain copy across the same logical boundary):** ~0.5-1.2 ns/op, 0 allocs.
- **Honest overhead statement:** the capability path costs ~11-27x a bare copy — but per *boundary transfer*, not per use; and 13 ns is far below any Go channel-send or task-spawn cost it would decorate. Per-byte on the guard-table wall W-014 measured (758 ns handle-table lookup), this is ~1.7% of one such operation.
- The check is **flag-insensitive**: no false sharing risk in the prototype (bit and payload in separate words; padding/layout for the real header-embedded variant is answer 1 below).

## 4. Direct answers to the three "review will press on" points (numbers, not prose)

1. **Header layout cost.** The prototype keeps a separate word today (live to keep `unsafe` out of a zero-dependency spike). For the header-embedded variant: `hmap` (`synchronized extra flags` chain) and `reflect.SliceHeader` both carry alignment padding that reclaims a bit at **0 extra bytes** per header — the proposal's claim, unchanged by these measurements.
2. **Ambient Go aliasing without a check-site.** Ambient reads never touch the bit: they cost the benchmark's *baseline* row (plain copy price). Only boundary ops (`send`/`task`/`move`) load/CAS the bit. An un-checked ambient alias therefore races *no differently from plain Go today* — but any attempt to make it cross a boundary fails closed, which is exactly the delta W-003 buys.
3. **Rust-bridge composition + W-011 erasure co-existence.** The pre-review clarification in [#193](https://github.com/limitlessprojects/Go-Hybrid/issues/193) already updates: `transferred-to-rust` is an epoch the Go side flips on `into_raw`, and the bit lives in the runtime container independent of the `.hy` lexical surface, so erasing `.hy` (W-011 family) does not erase the boundary check. No new evidence in this spike contradicts either; both remain stated as design answers, not endings.

## 5. Regression mapping (why this is *the* seam, not *a* seam)

| Landmark family in the Hall of Fame | What chased it frame-by-frame | Under this spike |
|---|---|---|
| #37/#44 const-write race (writes through a `const` root) | static const deep-immutability sweep | T1: the write attempt at the boundary is a second `Move` on a consumed bit → fail closed |
| PR #214..#218 slot-loan/loop-backedge stack | scope_pop sweeps, backedge re-freeze, closure-frame scoping, store-back flush points | T2: the whole freeze becomes one shared flag the first peer crossing sets |
| #110/#111/#123/#126 (the frame-ownership class) | "which frame owns this fact" walkers | removed by construction: the bit has exactly one owner-transition |

## 6. Limitations (boundaries of this spike)

- **L1:** The prototype models the *rule* of the bit, not the header embedding — pulling the `unsafe` header-reclaim variant off `hmap` padding is M1/M2 engineering, not spike scope.
- **L2:** `View[T]` storage-in-field is not enforced at runtime here (declared above in §2).
- **L3:** Benchmarks are desktop-AMD64, `-count=3`, Go 1.27.0; CI amd64/linux may differ by a few ns. No cross-OS sweep was run — that would be scope creep for a feasibility spike.
- **L4:** This is not a compiler patch; it cannot demonstrate the *syntax* (`x := ..., move y`) end-to-end. It demonstrates the runtime invariant the syntax would need.

## 7. Files

- `work/hybits.go` — prototype: `Own`/`Share`/`View` + the one-bit CAS boundary (116 lines, `std` only)
- `work/hybits_test.go` — T1-T4 regression map + 2 benchmarks (the falsifier harness)
- `RESULTS.md` — this file
