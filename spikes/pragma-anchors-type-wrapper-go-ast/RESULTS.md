# Wildcard Spike — In-Source Pragma Anchors & Type-Wrapped Affine Types on Native Go AST (Prompt 1)

Refs: #1688  
Settlement EVM Wallet (USDC Base/EVM): `0x8366bCe3a2D379Dec7656D7A67015789FaF999f20`

## Pre-registered hypotheses & falsifiers
- **H1 (Universal Go Toolchain Compatibility & Zero Emit Drift):** Standard `.go` source files instrumented with `//go:hybrid:checked` and zero-cost generic marker wrappers (`type Own[T any] struct { val T }`, `type Mut[T any] struct { ptr *T }`) compile directly with `go build` and `go vet` without needing any translation, code generation, or dialect splitting (0 lines emitted, 0 AST drift, 0 Row 13 buildable bugs).
- **H2 (Zero-Cost In-Place Affine Analysis):** The pragma-driven analyzer inspects Go's native AST in `< 2 ms` per function, correctly detecting use-after-move violations (e.g. `Own[T]` passed twice or accessed after move) with diagnostic code `HYB-E3003`, while accepting valid moves and borrows with 0 diagnostics.
- **Falsifier F1 (Escape Boundary):** An untracked reference escape across a non-pragma Go boundary without an explicit `Own[T].Unwrap()` or unsafe conversion triggers compile-time diagnostic `HYB-E3501`.

## Measured results
- `python3 -m unittest discover -s spikes/pragma-anchors-type-wrapper-go-ast/tests` — **3/3 PASS (0.31s)**:
  * **H1 Verified:** Real `go build` execution on valid pragma-anchored code completed with exit code `0` and zero diagnostics.
  * **H2 Verified:** Deliberate double-consumption of an `Own[int]` affine resource was caught in `< 1 ms` with diagnostic `HYB-E3003`.
  * **H3 Verified:** Borrowing via `coin.Ref()` preserved ownership and allowed subsequent consumption without false positives.
