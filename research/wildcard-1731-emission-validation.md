# Wildcard #1731 — You cannot build from `go/ssa`: keep source text, make invalid syntax unrepresentable

**Issue:** [#1731](https://github.com/limitlessprojects/Go-Hybrid/issues/1731) · **Status:** design-only; no compiler changes in this PR.

## 0. Verdict up front

The proposal has three parts: emit Go SSA instead of Go source, delete the CGo/Rust staticlib bridge, and thereby make "100% buildability by construction". The first part is not a compiler path — `golang.org/x/tools/go/ssa` is an **analysis IR**, and nothing in the Go toolchain consumes it as input. There is no `go build` flag, no linker input, and no stable serialization for it. Emitting SSA would mean writing our own backend from SSA to machine code, which is a multi-year project and a far larger source of bugs than the lexical class it claims to eliminate.

The real, measurable problem is different and much smaller: `hy-emit` builds Go source by string concatenation — **10,263 lines across 8 files, 253 `format!` sites, 35 `push_str` sites** — and then `hyb` pipes the result through `gofmt` (`compiler/crates/hyb/src/main.rs:420–428`). Sanitising that pipeline (`go/parser` + `go/types` on our own output, and an AST builder instead of string templates) kills the lexical-emission bug class without touching the backend, the ABI, or the zero-install story.

## 1. Why SSA emission is not a build path

- **`go/ssa` has no consumer.** The package builds a function-level IR in memory for analyses (vet, staticcheck-style tools). It is not accepted by `compile`, `link`, or `go build`, and there is no textual or binary serialization of it in the standard distribution. "Emit directly into Go SSA" therefore ends at a data structure in our process, not at a binary.
- **The alternative is our own backend.** To produce machine code we would need to lower SSA ourselves — register allocation, calling convention, GC write barriers, stack maps, interface dispatch, defer/panic tables. Every one of those is a correctness surface that Go's compiler currently owns for free, and none of them is smaller than the syntactic emission class.
- **gc's internal IR is not a contract.** Depending on `cmd/compile/internal/*` would pin Hybrid to one Go version's private structures and break the "zero-install, any Go toolchain" promise that Row 2 of the spike report validates (`go build` one command, 14/14).
- **"Buildability by construction" is a category error.** Emitting SSA does not remove the need to lower into something `link` accepts; it removes the *source parser* as a check on us and replaces it with a lowering we must test ourselves. The bug class moves; it does not disappear.

## 2. What source emission is actually buying

`hyb` runs `gofmt` over the emitted text (`main.rs:420–428`) — the artifact on disk **is** the interface. That buys three things the SSA route would forfeit:

1. **Debuggability.** `delve`, `go test -run`, pprof, and panic traces all point at `.go` files with line numbers. A SSA/bytecode path either loses source mapping entirely or rebuilds it by hand.
2. **Zero-install adoption.** The emitted module is ordinary Go; a user can read it, `git diff` it, and vendor it. This is the property the spike report's Row 2 measures.
3. **A second opinion from the toolchain.** `gofmt` normalises and rejects malformed text; `go build` then type-checks it. Both are free verification of our emitter, and both disappear when we become the backend.

Deleting `libhyrt.a` is a separate claim and mostly unsupported by the same reasoning: the staticlib is not only "ownership checks" — it carries the trampoline (`2.82 ns` sandbox / `4.36 ns` worker crossing) and the deep-stack pool. Reimplementing channels in Go is plausible; reimplementing the crossing at that cost with the same safety is a research programme, not a consequence of changing the emit target.

## 3. The salvageable proposal: make invalid emission unrepresentable

Two changes, both inside the existing pipeline:

1. **Structured emission.** Replace the 253 `format!`/35 `push_str` sites with a small Go AST builder (a Rust enum mirroring `go/ast`), printed by one printer. If the builder cannot express a token sequence, that sequence cannot be emitted — the lexical class is eliminated by type, not by review.
2. **Self-validation before write.** Pipe the printed source through `go/parser` (already reachable via the `go` toolchain on PATH, same discovery as `gofmt`) and report parse errors with the Hybrid span that produced the node. This converts "Row 13 BUILDABLE" bugs from downstream compile failures into precise emitter diagnostics.

Neither change touches `hy-check`, the Rust bridge, or the ABI.

## 4. Pre-registered tests (before any code)

1. **Zero parse errors on the corpus.** For all 129 files in `tests/checker-corpus/`, emitted source parses with `go/parser` and formats with `gofmt -e`. Any parse error is a bug found by the new gate; count them — that number is the size of the lexical class today.
2. **No build regressions.** `go build` success rate on the ten `examples/` programs stays at 10/10; `gofmt` output is byte-identical on the corpus (structured printer must reproduce current formatting).
3. **Debug parity.** A panic in an `examples/` program still resolves to a `.hy` line (via `//line` or an equivalent mapping) after the change, or the existing mapping is documented as unchanged.
4. **SSA falsifier.** If anyone wants to keep the SSA route alive, the bar is one end-to-end program: `.hy` → SSA → a binary that runs, using only a supported public Go toolchain path. If that cannot be demonstrated, the route is not a route.
5. **Cost.** Structured emission must not slow `hyb` by more than 5% on the corpus build, and must not change emitted size by more than 2%.

## 5. What does not survive

- **"Emit Go SSA"** as a build path — no consumer exists; it is an analysis IR.
- **"100% buildability by construction"** — lowering replaces parsing; the bug class moves to us.
- **Deleting `libhyrt.a` as a side effect** — the trampoline and stack pool are not ownership checks, and their replacement is its own project.
- **Losing source text** — it is what `gofmt`, `delve`, `git diff`, and zero-install all depend on.

**What survives:** judge the emitter by its own output, in-process, before anything reaches disk.
