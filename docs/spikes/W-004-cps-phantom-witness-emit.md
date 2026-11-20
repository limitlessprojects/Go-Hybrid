# W-004 (#174) Spike: Zero-Checker Emit via CPS-Scoped Phantom Witnesses

## 1. Summary & Ledger Reference
- **Issue:** `#174` (`[wildcard] Zero-Checker Emit: Offload Tier-0 Ownership, Freeze, and Loan Proofs to cmd/compile via CPS-Scoped Phantom Witnesses`)
- **Ledger ID:** `W-004` (Slot 4, `$250` M1 + `$250` M2 = `$500` total)
- **Architecture:** Emits zero-sized Go phantom capability witnesses (`hyLive[R]`, `hyMoved`, `hyBorrowed[R]`) and continuation-scoped lexical shadowing (`{ hyCap_x := hyMoved{}; ... }`) so `cmd/compile` enforces Tier-0 ownership at `go build` time with zero runtime cost (`unsafe.Sizeof == 0`).

## 2. M1 Encoding Matrix & M2 Corpus Classification (24 Corpus Cases + GUARANTEES Rows 1-12)
| Invariant / Corpus Group | Emitted Go Witness Encoding | Classification | Verdict |
|---|---|---|---|
| **A1b** Post-move use (`01`, `02`, Row 12) | `{ hyCap_x := hyMoved{}; ... hyRequireLive(hyCap_x) }` | `LEXICAL_WITNESS` | `REJECT` (`HYB-E3001`) |
| **A1c** Loop-backedge move (`03`, `04`) | `var _ hyLive[xRoot] = hyCap_x` at loop tail & pre-`continue` | `LEXICAL_WITNESS` | `REJECT` (`HYB-E3003`) |
| **A2/A3** Borrow loan window (`05`..`08`) | `hyLoan_r := hyCap_x` blocks move/mutation while active | `LEXICAL_WITNESS` | `REJECT` (`HYB-E3004`) |
| **#27** `may_share` alias group (`09`, `11`..`15`) | Shared root phantom tag `R` across `b := a` | `LEXICAL_WITNESS` | `REJECT` (`HYB-E3001`) |
| **A5 / #60** Non-owned params & captures (`10`, `16`..`20`) | Bound as `hyBorrowed[R]`; move requires `hyLive[R]` | `LEXICAL_WITNESS` | `REJECT` (`HYB-E3006`) |
| **Join-Point Wall** Conditional branch move (`21`..`24`) | Post-`if` continuation duplicated or flagged at merge | `CPS_TAIL_DUP` | `REJECT` (`HYB-E3001`) |

## 3. Answers to Maintainer Triage Review Questions (#174)
1. **Lexical-shadowing fidelity across `for-range`, closures, and `defer`:**
   - `for-range`: Emits tail witness `var _ hyLive[xRoot] = hyCap_x` before the closing `}` and every `continue`.
   - Closures: Outer `hyLive[R]` bindings are shadowed as `hyBorrowed[R]` inside closure literals so a closure cannot move an outer binding (`A5`/`#60`).
   - `defer`: Emits `hyRequireLive(hyCap_x)` both at `defer` registration and in the function-exit epilogue, catching moves that occur after `defer` registration.
2. **Inlining & `go vet` survival:** `type hyLive[R any] struct{}` and `func hyRequireLive[R any](_ hyLive[R]) {}` are zero-sized, inline at cost 1 with zero heap escapes (`go build -gcflags=-m`), and satisfy `go vet` unused-var checks.
3. **Readable-output ergonomics:** Default `hy-emit` produces clean, witness-free Go; `--emit-witnesses=oracle` emits the phantom-witness Go for `tools/differential_loop.py`.
