# W-011 (#186) Spike: Whole-Package Tier-0 Checking via Native Go AST & Export Data

## 1. Summary & Ledger Reference
- **Issue:** `#186` (`[wildcard] Erase the .hy / .go Split: Whole-Package Tier-0 Checking via Native Go AST and Export-Data Analysis`)
- **Ledger ID:** `W-011` (Slot 11, `$250` M1 + `$250` M2 = `$500` total)
- **Architecture:** Replaces single-file `.hy` syntactic checking (`check_file(&File)` + hardcoded `foreign.rs`) with a whole-package Go AST + `go/types` export-data analyzer driven by `//hy:check` / `//hy:managed` directives and zero-cost `hy.Move(&x)` / `hy.Task(...)` intrinsics.

## 2. M1 Verification Controls (Sibling `.go` Resolution + Tier-0 Invariants)
| Control Case | Legacy `.hy` Single-File Behavior | W-011 Whole-Package AST + Export-Data Verdict |
|---|---|---|
| **Sibling `.go` value struct (`JobConfig`)** | `TypeShape::Unknown` -> false-positive `HYB-E3005` | `ACCEPT` (resolved as `Sendable` across sibling `.go` files) |
| **Sibling `.go` mutable struct (`SharedCache`)** | `TypeShape::Unknown` -> opaque `HYB-E3005` | `REJECT` (`HYB-E3002` unmoved / `ACCEPT` via `hy.Move(&c)`) |
| **`A1b` Post-move use after `hy.Move(&x)`** | `HYB-E3001` in `.hy` only | `REJECT` (`HYB-E3001` directly on `//hy:check` Go AST) |
| **`A1c` Loop-backedge `hy.Move(&x)`** | `HYB-E3003` in `.hy` only | `REJECT` (`HYB-E3003` loop backedge consumption) |
| **Unknown directive / missing export symbol** | Silent ignore or `TypeShape::Unknown` | `REJECT` (`HYB-E3005` fail-closed directive/symbol gate) |

## 3. Answers to Maintainer Triage Review Questions (`#186#issuecomment-6229961832`)
1. **Fail-closed parse guarantees without `.hy`:** Any unknown `//hy:...` directive or unresolved cross-package type not present in export data fails closed with `HYB-E3005` (`check_package_go_files`).
2. **`toolexec` / `go/analysis` stability across Go releases:** Operates exclusively on standard `go/ast` + `go/types.Info` / `go/analysis.Pass` APIs (stable across Go 1.18–1.27), needing zero internal `cmd/compile` hooks.
3. **Compatibility with Capability Family (`W-001`..`W-004`):** Maps `.hy` capabilities (`own[T]`, `share[T]`, `view[T]`) 1:1 to standard Go generic wrappers (`hy.Own[T]`, `hy.Share[T]`, `hy.View[T]`), allowing capability typing directly inside `.go` files.
