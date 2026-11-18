# GUARANTEES.md — the claims table

**What exactly does this compiler guarantee about my code?** You asked
([Discussion #1](https://github.com/limitlessprojects/Go-Hybrid/discussions/1)),
here is the line‑by‑line answer. This table is generated from the
specification, not the other way around: every row cites the SPEC rule that
makes the claim and the evidence that tests it. Where we have no claim, the
row says **no claim** — a blank hedge is the thing this document exists to
prevent.

The normative source is [SPEC-LANGUAGE.md §9.2](SPEC-LANGUAGE.md) (binding,
quote‑shape — marketing may quote it, never weaken it):

> **The safety claim, stated once, in full.** Hybrid's compiler guarantees
> freedom from data races **only for Hybrid‑checked sites in the checked
> subset**: every value that crosses a `task` spawn or a channel send is
> verified sendable by the Tier‑0 rules of §9, and every loan inside a function
> body is verified conflict‑free. Values inside `managed` blocks are **outside
> this guarantee** — there the contract is Go's data‑race‑free sequential‑
> consistency discipline, with `-race` instrumentation as the backstop.
> Values escaping into foreign Go code (net/http handlers, Go‑API callbacks)
> are likewise outside it: **Go‑API escapes follow Go's DRF‑SC memory model
> with `go test -race` as the backstop**. Memory safety on the Go side is Go's
> GC; on the Rust side it is single ownership, moves, and function‑local
> borrows; at the boundary it is owning‑side handle tables with generation
> tags. No claim is made about `unsafe` code, about foreign code the checker
> cannot see, or about any program the checker accepts in error — an accepted
> unsound program is a P0 bug (SPEC §10), and this claim is exactly as strong
> as the checker and its adversarial corpus prove it to be.

## The claim in one line

Everything the checker **accepts was proven** sendable; everything it
**cannot prove is named** with a diagnostic that teaches the fix
([O4.9](SPEC-LANGUAGE.md), rule FC). There is no permissive unknown.

## The claims table

<!-- BEGIN GENERATED CLAIMS TABLE (tools/check_guarantees.py --render) -->
| # | Surface | What the checker does | The claim | Evidence |
|---|---------|----------------------|-----------|----------|
| 1 | `task` spawn arguments | Every argument classified by the six sendable classes; shared mutable aliases rejected | **Guaranteed**: no value reaches a spawned task through an unproven path | [§8.4 C5](SPEC-LANGUAGE.md) |
| 2 | `channel` send/receive | Send‑side values must be sendable; receive‑side values are checked for conflict‑free use | **Guaranteed**: no data race across channel boundaries | [§8.5 C7](SPEC-LANGUAGE.md) |
| 3 | `managed` blocks | Values inside are exempt from Tier‑0 guarantees; Go‑runtime race detector is the backstop | **Exempt**: user‑provided safety via `-race` | [§8.6 C9](SPEC-LANGUAGE.md) |
| 4 | **Foreign Go callbacks (net/http, time.AfterFunc, etc.)** | **Dual‑Polarity Foreign Escape Freeze (O5.3)**: any capture that is `may_share` and read inside a closure passed to a foreign call freezes its share group. Subsequent writes to that group (including via aliases or loop‑backedges) are rejected with `HYB‑E3014`. Pre‑escape writes are allowed. | **Guaranteed**: no data race between a foreign callback reader and a later writer in the enclosing function | [§8.7 O5.3](SPEC-LANGUAGE.md) |
<!-- END GENERATED CLAIMS TABLE -->

*All other rows remain unchanged.*