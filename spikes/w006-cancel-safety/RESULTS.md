# W-006 spike — cancellation-safety as one more pass over Tier-0 facts

Ledger: **W-006** (issue #352, proposal by @raju12345-raju). This spike is the
"executable artifact" the ledger row says is missing: a real pass inside
`hy-check`, run on the real corpus, answering the three questions the triage
comment says review will press on — what the pass does at **loop back-edges
and `select`**, the **exact guarantee text** (row-16-grade precision), and the
**cost on the corpus**.

## Status

- [x] Hypothesis + expected case verdicts pre-registered (this section, committed before the pass exists)
- [ ] Pass implemented (`compiler/crates/hy-check/src/cancel.rs`, `hyb check --report-cancel | --deny-cancel`)
- [ ] Cases run, raw results appended below
- [ ] Corpus cost measured
- [ ] Falsifier hunt results appended

## Ground truth established before designing (run on main @ 0ca08c2)

1. `import "context"` is not in the curated surface (SPEC-LANGUAGE M5); a
   `context.with_cancel` program fails closed at `HYB-E3005`. So in checked
   Hybrid **`close(ch)` is the only cancellation primitive** — and Go's
   `close` unblocks *receivers* only, never senders (a send on a closed
   channel panics; a blocked send stays blocked until a receiver arrives).
2. A worker that ignores its closed `done` channel and blocks on `<-jobs`
   is accepted today with 0 diagnostics — the falsifier the issue asks for
   exists against the *current* checker, as expected (Tier-0 makes no
   liveness claim). The proposal stands or falls on whether a pass can
   *name* every such shape.

## Definitions (the pass, stated before it is written)

- **Stable binding** — a name declared exactly once in a function body
  (parameter or one local declaration, no shadowing anywhere in the body)
  and never the target of an assignment.
- **Cancellation token** — at a spawn `task f(args)` in function `g`, argument
  `i` is a token iff it is a bare stable identifier `x` in `g` and either
  `close(x)` appears in `g` (statement, `defer`, or a closure inside `g`), or
  `x` is itself a token of `g`. The token set of `f` is its stable params at
  token positions. Ordinary calls map tokens to callee params the same way.
- **Blocking site** — a channel send, a channel receive (any expression
  position), `for … in` over a channel, a `select` without `default`.
- **Cancel-observable site** (in a task context with token set `T`):
  a receive from `t ∈ T`, a `for … in t`, a `select` with a receive arm on
  `t ∈ T` or on a timer (`time.after(…)` directly or a stable local bound to
  it), or any `select` with `default`. Every other blocking site is
  **cancel-oblivious** → `HYB-E8002` (lint, never fails the build;
  `--deny-cancel` turns any into a failed check by policy, exactly as
  `--deny-managed` does for `HYB-E8001`).
- **Reach** — roots are `main`, `init`, and `pub fn`s, analyzed in *root
  mode* (nothing reported: the root task is not cancellable). A spawn
  switches to *task mode* with the mapped token set. Calls keep the mode.
  Contexts are memoized on `(fn, token set, mode)`.
- **Callees** — a file-level fn: analyzed under the mapped tokens. A method
  `x.m(…)`: every method named `m` in the file (union). A local bound to a
  closure literal: covered (closure bodies are walked inline in the defining
  context). A foreign package call: **outside the claim** (counted, not
  reported) — `time.sleep` is bounded. Anything else (a `fn`-typed param, a
  closure from a call): **fail-closed** → `HYB-E8002`.
- `managed` bodies and closure literals passed directly to foreign calls are
  outside the claim (not walked).

## Answers the review asked for, pre-committed

- **Loop back-edges.** Token-ness is a flow-insensitive *must* property of
  stable bindings, so no back-edge can change it, and each blocking site is
  classified locally. No intra-procedural fixpoint is needed: that is *why*
  one pass suffices. The only fixpoint is inter-procedural over a finite
  lattice (subsets of a fn's params), monotone (more tokens ⇒ fewer findings).
- **`select`.** A `select` is one blocking site, not one per arm: it is
  cancel-observable iff it has `default` or at least one token/timer receive
  arm. Send arms are fine *inside* such a select (cancellation picks the
  token arm); a bare send never is.
- **Proposed row-12 text** (to be confirmed or killed by the results):
  > **Checked subset, opt-in (`hyb check --deny-cancel`).** Call a `chan`
  > argument of a `task` spawn a *cancellation token* when the spawning
  > function `close`s that same binding (declared once, never reassigned)
  > or itself holds it as a token. Once every token of a spawn has been
  > closed, no checked channel operation reachable from that task —
  > through resolved Hybrid callees, closures defined in checked code, and
  > nested spawns — blocks forever: every send, receive, `for … in` over a
  > channel, and `select` without `default` is a token receive, a `select`
  > with a token- or `time.after`-receive arm, or is reported `HYB-E8002`.
  > **No claim** inside `managed`, inside foreign calls (`sync` waits,
  > `net/http`, blocking Rust calls), for tasks spawned with no token, for
  > `main`/package entry points, about *whether* the token is ever closed
  > (cancellation liveness), or about non-blocking divergence.

## Pre-registered hypotheses

- **H1 (soundness on the cases).** Every case below gets exactly the
  verdict written in its `// EXPECTED-CANCEL:` header.
- **H2 (no regression).** The pass is opt-in: `cargo test` and the corpus
  harness stay exactly as on main (79/79), no default output changes.
- **H3 (cost).** On all corpus programs + examples, the pass adds < 5% to
  `hyb check` wall time (median of 5), and its context count is ≤ 2× the
  number of task-reachable functions.
- **H4 (signal).** On the existing corpus/examples, findings are real
  leak shapes (worker result-sends with no cancel path), not noise from
  unresolved types. Each finding is classified by hand below.
- **Falsifier (from the issue).** One program in checked code (no
  `managed`, no foreign calls) where a task blocks forever after all its
  tokens are closed and `--deny-cancel` passes. Any such program kills the
  row-12 text above and becomes a regression case.

## Cases (`cases/`, verdicts written before the pass existed)

| Case | Shape | Expected |
|---|---|---|
| p1-select-token-arm | `loop { select { <-jobs / <-done: return } }` | clean |
| p2-for-in-closed-jobs | `for j in jobs` where spawner closes `jobs` | clean |
| p3-select-timer-arm | `select` with a `time.after` arm | clean |
| p4-select-default | non-blocking `select` with `default` | clean |
| p5-nested-spawn-token | task re-spawns a child passing its token | clean |
| p6-callee-token | task calls helper(done) that selects on `done` | clean |
| p7-main-blocks | `main` blocks on a bare receive (root not claimed) | clean |
| p8-managed-excluded | bare receive inside `managed` (outside claim) | clean |
| n1-bare-recv-ignores-done | `<-jobs` while spawner closes only `done` | 1 × E8002 |
| n2-result-send-leak | worker `results <- r` (close never unblocks a sender) | 1 × E8002 |
| n3-token-reassigned | `done = make(chan bool)` inside the task kills the token | 1 × E8002 |
| n4-shadowed-close | spawner closes a *shadowing* `done` | 1 × E8002 |
| n5-callee-no-token | helper blocks on a channel the caller did not tokenize | 1 × E8002 |
| n6-closure-blocks | closure literal invoked in task blocks on non-token | 1 × E8002 |
| n7-fn-param-callee | task invokes a closure returned by a call (fail-closed) | 1 × E8002 |
| n8-alias-not-tracked | `d := done; task w(d); close(done)` (documented conservative FP) | 1 × E8002 |
| n9-select-no-token | `select` with two non-token receive arms | 1 × E8002 |

Amendment (before the pass existed): n7 originally passed a `fn` value as a
spawn argument; Tier-0 already rejects that (`HYB-E3005`), so the unresolved
callee is now built inside the task. n9's arms gained braces (the parser does
not accept two consecutive unbraced `name := <-ch:` arms). All 17 cases are
accepted by Tier-0 on main, so only the new pass decides their verdict.

## Raw results

*(appended after running — nothing below this line was written before the run)*
