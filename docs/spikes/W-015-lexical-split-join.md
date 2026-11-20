# W-015 (#294) Spike: Lexical Split-Join Slice Capabilities via 3-Index Capacity Clipping

## 1. Summary & Ledger Reference
- **Issue:** `#294` (`[wildcard] Lexical Split-Join Slice Capabilities via Zero-Allocation 3-Index Capacity Clipping`)
- **Ledger ID:** `W-015` (Slot 15, `$250` M1 + `$250` M2 = `$500` total)
- **Architecture:** Lowers `split s at mid into left, right { ... }` to Go's 3-index full-slice expressions (`left := s[0:mid:mid]`, `right := s[mid:len(s):cap(s)]`), enforcing `cap(left) == len(left) == mid` so `append(left, x)` reallocates onto a fresh backing array instead of overwriting `right[0]`.

## 2. M1 Triage Review Answers & Soundness Falsifiers (`#294#issuecomment-6228661273`)
| Triage Question / Falsifier | Rule Enforced in `split_verifier.py` | Verdict |
|---|---|---|
| **Q1 `append`-into-clip (`s[0:mid:mid]`)** | `cap(left) == mid` -> `append` reallocates fresh array; `right[0]` untouched | `ACCEPT` (0 race) |
| **Q2 `split` block scoping & post-block restoration** | `s` blocked inside (`HYB-E3106`), restored after `}`; fork escape after `}` rejected (`HYB-E1001`) | `REJECT` (`HYB-E3106` / `HYB-E1001`) |
| **Q3 Nested `split`-in-`split`** | Recursive `ll := left[0:q:q]`, `lr := left[q:len(left):cap(left)]` inherits `cap(left) == mid` | `ACCEPT` (3 disjoint tasks) |
| **F1a/F1b Non-owned param & `const` slice** | Splitting borrowed parameter or `const` slice rejected before minting forks | `REJECT` (`HYB-E3105`) |
| **F2 Reference-bearing elements (`[]map[K]V`)** | Requires value-shaped element type `T`; rejects interior heap references | `REJECT` (`HYB-E3105`) |
