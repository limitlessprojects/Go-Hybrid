# W-006 spike — cancellation-safety as one more pass over Tier-0 facts

Ledger: **W-006** (issue #352, proposal by @raju12345-raju). This spike is the
"executable artifact" the ledger row says is missing: a real pass inside
`hy-check`, run on the real corpus, answering the three questions the triage
comment says review will press on — what the pass does at **loop back-edges
and `select`**, the **exact guarantee text** (row-16-grade precision), and the
**cost on the corpus**.

## Status

- [x] Hypothesis + expected case verdicts pre-registered (this section, committed before the pass exists)
- [x] Pass implemented (`compiler/crates/hy-check/src/cancel.rs`, `hyb check --report-cancel | --deny-cancel`)
- [x] Cases run, raw results appended below (pinned by `hy-check/tests/cancel_cases.rs`)
- [x] Corpus cost measured
- [x] Falsifier hunt results appended (3 rounds; 8 falsifiers found and fixed)

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

### R1 — pre-registered cases (H1): 17/17 on pass v1

Every case got exactly its pre-registered verdict on the first run, at the
intended site (n1 flags `<-jobs`, not `<-done`; n9 flags the `select`, etc.).
`--deny-cancel` fails the check (exit 1) whenever a finding exists.

### R2 — falsifier hunt, round 1: three falsifiers against pass v1

All three are accepted by Tier-0, use no `managed` at the blocking site and
no foreign blocking call, block forever after `done` is closed, and got
**0 findings** from v1 (committed as failing cases before the fix):

| Case | Hole in v1 | Fix (v2) |
|---|---|---|
| f1-timer-reused-across-backedge | `time.after` fires **once**; a timer bound outside a loop and selected on inside it is drained after the first timeout, so the next iteration can block forever. v1 counted any stable timer local as bounded. | A `time.after` local bounds a wait only inside the loop/closure body that bound it; a fresh `time.after(…)` call in the arm, or periodic `time.tick`, still counts. **This is the answer to "what does the pass do at loop back-edges":** token-ness needs no back-edge reasoning (flow-insensitive must property), but *timer liveness does* — it is the one place a back-edge changes a site's classification. |
| f2-spawn-inside-managed | v1 skipped `managed` bodies entirely, so a checked `task` spawned in one was never discovered; the spawned body is ordinary checked code. | `managed` bodies are walked in root mode: their own sites stay outside the claim, spawns inside them are analyzed. |
| f3-spawn-inside-foreign-callback | Closures handed to foreign APIs were skipped, hiding spawns inside them. | Same root-mode walk. |

### R3 — falsifier hunt, round 2 (v2 → v3)

| Case | Result |
|---|---|
| g1-task-local-close-after-wait | flagged — a task-local `close` is *not* a token for its own callees (otherwise a self-deadlock is certified); tokens flow into ordinary calls only from the caller's own token set. |
| g2-spawn-closure-local, g3-spawn-method | Tier-0 already rejects both shapes (HYB-E3001 / E3005); the pass flags them anyway (kept as defense-in-depth). |
| g4-closure-via-struct-field | **Not a falsifier, and why is a learning:** v2 misclassified `h.step()` (fn-typed field, no such method) as *foreign*, yet still flagged the receive — at the closure's *definition*, walked inline because `make_hooks` runs in task mode. That is sound only because **Tier-0 rejects closures at both check-sites and Hybrid has no package-level vars, so a closure only ever runs in a task that created it** — premise 1 of the proposal ("spawn graph finite and visible") is doing real work here. v3 also fails closed at the call site (2 findings). |
| g5-deferred-closure-blocks | flagged. |

### R3b — falsifier hunt, round 3: independent code review (v4 → v5)

An independent review of `cancel.rs` proposed 7 holes; each was written as a
program and run. 5 were real (accepted by Tier-0, silent or mis-attributed
under v4), 2 are not expressible (Tier-0/parser reject them):

| Case | Hole in v4 | Fix (v5) |
|---|---|---|
| h1-recv-in-index-target | `m[<-jobs] = 1`: expressions inside assignment targets were never walked. | Walk index/field bases in targets. |
| h2-for-in-method-chan, h3-for-in-local-from-method | `for … in` over a method's channel result (directly or via a local) was treated as non-channel. | `may_be_chan` resolves method results; unknown locals fail closed (element-like bindings still gated on nested chan types). |
| h4-timer-waited-twice | One `time.after` waited on twice in the same body: the second wait is on a drained timer. | A once-timer is bounded only if it has exactly one wait site in the fn (the first select becomes a conservative FP). |
| h5-named-args-swapped | Named args were mapped positionally, certifying the wrong param as the token. | Named or spread args map no tokens (fail closed). |

Not reproducible: channel type aliases (`make(jobq)` → HYB-E3005) and writes
through `&done` (parse error). Corpus findings stay at 6 under v5 (no new noise).

### R4 — cost on the corpus (H3): confirmed

All 90 programs (79 corpus + 11 examples), release build, median of 5 runs of
`hyb check` per file, Apple M-series:

| | wall (90 processes) |
|---|---|
| `hyb check` | 312 ms |
| `hyb check --report-cancel` | 309 ms (−0.9 %, i.e. within noise) |

182 contexts were analyzed for 184 fn declarations (H3 bound: ≤ 2×). The
pass is linear in body size per context, and on this corpus nearly every
function is analyzed in exactly one context.

### R5 — signal on the corpus (H4): REFUTED as pre-registered — one precise gap

**Deviation found by the corpus run.** v3 reported 58 findings, 52 of them
in tasks holding *no* token — but the pre-registered claim text says "no
claim … for tasks spawned with no token". The implementation was
over-reporting relative to its own claim. Amended scope (v4, logged here
after seeing the data): a task is in scope when it holds a token, when its
spawner closes *some* channel (cancellation was intended), or when an
in-scope task spawned it (so children of cancellable tasks cannot escape).
Out-of-scope sites are counted, not reported. All 25 case verdicts are
unchanged under v4 (n3/n4/n8 stay flagged: their spawner closes `done`).

Under v4 the corpus has **6 findings**, classified by hand:

| File | Site | Real leak? |
|---|---|---|
| examples/03-worker-pool | `results <- …` | no — `results` cap 16 ≥ 8 total sends |
| examples/gomod-integration/calc.hy | `out <- …` | no — `out` cap = shards, one send per shard |
| corpus 03 / 04 / 18 | `done <- true` | no — `done` cap = n_workers, one send per worker |
| corpus 16 | `acks <- b.seq` | no — `acks` cap 8 = n_batches |

**All six are the same false-positive class: capacity-bounded sends** (the
one-slot-per-sender idiom). None is a real leak in these programs. The pass
is sound but does not count sends against buffer capacity; closing that gap
needs a per-channel send-count bound (sends outside loops × spawn count ≤
`make` capacity), which is a separate, well-scoped follow-up.

### R6 — regression check against upstream (H2): confirmed

- `cargo test --workspace`: 1144 passed, 0 failed (pinned go1.26.9).
- Corpus harness 79/79; managed gate PASS; differential loop FULL AGREEMENT
  (23/23 build+run, 56/56 reject); `run_examples.sh` 10/10 PASS.
- Default output is byte-identical to upstream `main`: 381 runs (127 files ×
  plain / `--json` / `--report-managed`), 0 differences in stdout, stderr or
  exit code.

## What this means for row 12

The claim survives three adversarial rounds (two self-hunts, one independent
review) **with these amendments, each one a published learning**:

1. *Back-edges matter for timers, not for tokens* (f1). Token-ness is
   back-edge-invariant; single-shot timer liveness is not.
2. *Spawn discovery must see through `managed` and foreign callbacks* (f2,
   f3): "outside the claim" applies to a region's own sites, never to the
   checked tasks it spawns.
3. *Scope is "cancellation intended"*, not "every task" (R5): otherwise the
   lint drowns in tasks nothing ever cancels.
4. *Every expression position and every callee shape must fail closed*
   (R3b): assignment targets, method-returned channels, repeated waits on a
   one-shot timer, and named/spread arguments each hid a blocking site.

Proposed row-12 text after the amendments (replaces the pre-registered draft):

> **Checked subset, opt-in (`hyb check --deny-cancel`).** Call a `chan`
> argument of a `task` spawn a *cancellation token* when the spawning
> function `close`s that same binding (declared once, never reassigned) or
> itself holds it as a token. For every spawn whose spawner holds or closes
> a token (and every task such a task spawns), once every token is closed no
> checked channel operation reachable from that task — through Hybrid
> callees in the same file, closures defined in checked code, and nested
> spawns, including spawns inside `managed` blocks and foreign callbacks —
> blocks forever: every send, receive, `for … in` over a channel, and
> `select` without `default` is a token receive, a `select` with a token
> receive, a `time.tick` arm, or a `time.after` arm bound in the same loop
> iteration and waited on exactly once, or it is reported `HYB-E8002`. Calls
> through values whose body is not visible, and token mapping through named
> or spread arguments, fail closed. **No claim** inside `managed` blocks' own
> sites, inside foreign calls (`sync` waits, `net/http`, blocking Rust
> calls, other packages), for tasks nothing cancels, for `main`/package
> entry points, about *whether* a token is ever closed (cancellation
> liveness), or about non-blocking divergence. Known imprecision:
> capacity-bounded sends are reported (R5).

**Not claimed, and the honest next step:** this is a per-spawn *conditional*
guarantee. A child whose token its parent never closes on some exit path is
outside it — that transitive "every exit closes every token" property is
structured cancellation, i.e. W-007/W-008 territory, which this pass would
compose with rather than replace.

## Reproduce

```bash
cd compiler && cargo test -p hy-check --test cancel_cases
cargo build -p hyb && for f in ../spikes/w006-cancel-safety/cases/*.hy; do target/debug/hyb check "$f" --report-cancel; done
```
