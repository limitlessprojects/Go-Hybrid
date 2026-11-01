# M1 Preregistration — W-010 / #184 Session-Type Endpoint Generation

**Author:** TNT agent
**Date registered:** 2026-10-19 (BEFORE first test run)
**Issue:** https://github.com/limitlessprojects/Go-Hybrid/issues/184 (slot W-010)
**Ledger:** MOONSHOT.md slot 10
**Preregistered BEFORE first test execution:** YES

## Hypothesis

For an explicitly closed subset of protocols described as a **global type**
(finite labelled message sequence with branch, no recursion, no delegation,
no dynamic spawn), the following holds:

> If a global type `G` projects to every role under Honda-Yoshida-Carbone
> projection (with the standard uninvolved-role merge condition), then the
> projected local endpoints are communication-deadlock-free, conditional on
> terminating local computation and fair scheduling.

And its contrapositive (the sharp test):

> The canonical naive local traces
>   A: send_B(p); recv_B(q)
>   B: send_A(q); recv_A(p)
> **cannot** be synthesized into any global type. There is no global type
> that projects back to both local types.

## What this spike is

Standalone, dependency-free Rust crate (~400-600 LOC):

- `Global` type: `End | Msg { from, to, label, cont } | Choice { at, branches }`
- `Local` type: `End | Send { to, label, cont } | Recv { from, label, cont } | Select { to, branches } | Branch { from, branches }`
- `project(g, role) -> Result<Local, ProjectError>` with uninvolved-role merge
- `synthesize(roles: Map<Role, Local>) -> Result<Global, SynthError>` (inverse)
- `codegen_go(g) -> String` producing runnable Go with a hub mediator
- `simulate(g) -> SimResult` **exhaustive interleaving explorer**:
    - Models each projected endpoint as a step-function over unbuffered channels.
    - Enumerates ALL schedules of runnable endpoints up to a bounded depth.
    - Reports `Terminates` (all endpoints reach End) or `Deadlock` (some
      reachable schedule has no runnable endpoint while some endpoint is
      unfinished) with a minimal witness trace.

## Success bar (all must hold for M1 to stand)

1. All unit + integration tests pass on `cargo test --release`.
2. `synthesize({A: send p; recv q, B: send q; recv p})` returns `Err`, with
   a published reason (no role can fire first / cycle detection).
3. `synthesize({A: send p; recv q, B: recv p; send q})` returns the global
   type `A -> B : p . B -> A : q . End`, and projection back to A and B
   round-trips.
4. `simulate(that global)` returns `Terminates` and the exhaustive search
   found every endpoint completed.
5. `codegen_go` for (3) produces a syntactically valid Go source (verified by
   inspection; **no `go run` — Go toolchain not installed on author machine**
   — see "Honest limits").
6. The crate builds offline: `cargo build --offline` succeeds (no crates.io).

## Kill criteria (falsify the hypothesis, publish the wall)

- If (2) does **not** return `Err` — i.e. the A/B ping-pong is somehow
  synthesizable — the synthesis direction is falsified for this subset.
  Publish the witness global type.
- If (4) reports any `Deadlock` for a well-formed global, the "well-formed
  ⇒ deadlock-free" claim is falsified for this subset. Publish the trace.
- If `cargo test` cannot pass offline, the dependency-free claim is
  falsified. Publish `cargo tree`.
- If `codegen_go` output, when compiled by a real `go build` (by the
  reviewer, not by me), does not compile, the codegen claim is falsified.
  I accept this on the reviewer's authority — I cannot run Go here.

## Honest limits (stated up-front, not after failing)

- **No Go toolchain on author machine.** `codegen_go` is inspected by
  hand. The *semantics* of the generated Go are validated by `simulate`,
  which models the same scheduling rules (unbuffered channels, blocking
  send/recv, round-robin fair scheduler). A reviewer with Go installed
  should run `go build ./work/` and `timeout 5 go run ./work/` as a
  confirmation; that is a check I cannot perform.
- No recursion (`mu X . ...`).
- No endpoint delegation.
- No `select`-on-multiple-channels; one channel per (sender, receiver) pair.
- No async cancellation, no fairness under preemptive scheduling.
- No foreign code, FFI, or the Rust bridge (this is a Go-only demo).

## Cost

~4-6 hours of agent time. If the hypothesis fails, the smallest
counterexample or the exhaustive deadlock trace is published here, not in
the PR's "highlights".
