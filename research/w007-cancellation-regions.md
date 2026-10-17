# W-007 — Certified cancellation-safe structured task regions (issue #348)

Status: **design analysis (M1 input), no compiler code.** Nothing below is
implemented or measured. Every "would" is a hypothesis with a stated way to
fail (Prime Directive 1/2). Targets GUARANTEES.md rows 12 and 16; keeps the
existing sendability analysis (O4–O6, C5) untouched — this adds a
control-flow/effect contract on top, it does not replace alias/loan/freeze
(contrast W-001/#304).

## 1. Property and fault model (preregistered test 1)

**Region.** An opt-in block `region(ctx) { ... }` (surface syntax is
provisional) in which `task` spawns are lexically scoped to the block.

**Certified property P** — for every execution of an admitted region under
the fault model below:

- **P1 (delivery)** once the region's cancel signal is set, it is observable
  by every task transitively spawned in the region;
- **P2 (exit)** every potentially blocking operation executed by those tasks
  has a cancellation exit, and every loop that does not otherwise terminate
  has a cancellation check on its backedge, so each task reaches its end in
  finitely many scheduler steps after the signal;
- **P3 (join)** the region's exit point happens-after every child's
  termination (no orphan child);
- **P4 (cleanup order)** region-owned resources (channels, `defer`red
  closers, buffers) are released only after P3, so no child accesses them
  after release.

**Fault model (in scope):** cooperative cancellation, fair scheduler, no
panics escaping a child except through the supervisor (below), callees fully
summarized.

**Out of scope (explicit non-claims):** general deadlock freedom (a region
can still deadlock *before* cancel — P only says that once cancelled, it
drains); deadlines/timers as a correctness property; process crash/`os.Exit`/
`runtime.Goexit` outside the supervisor; arbitrary external side effects
(a child cancelled mid-write leaves a partial write); non-terminating
*pure compute* with no backedge check (rejected, not tolerated); the
fairness assumption itself (Go's scheduler is only practically fair).

## 2. Cancellation-effect lattice and summaries

Each function gets a summary `eff(f) ∈ {Pure, Obs, Blocks(¬obs), Unknown}`
plus a boolean `joinsAll` / `spawns` set:

| Summary | Meaning | Admissible in region |
|---|---|---|
| `Pure` | terminates, no blocking ops, loops bounded or checked | yes |
| `Obs` | every blocking op / unbounded loop reachable observes the region signal | yes |
| `Blocks` | some blocking op has no cancel exit (plain `<-ch`, `sync.Mutex.Lock` held across wait, `time.Sleep` w/o select, `wg.Wait`) | **no** |
| `Unknown` | unresolved callee, interface/method value, computed callee, foreign/`managed` call | **no** |

Composition: `eff(f) = ⊔ over call sites`, with `Unknown` absorbing and
`Blocks` > `Obs` > `Pure`. Recursion is solved by fixpoint over the call
graph SCC, starting at `Pure`; an SCC with an unbounded cycle that has no
check on a cycle edge is `Blocks`. This reuses the **fail-closed** posture
already in rule FC / `HYB-E3005` (SPEC-LANGUAGE §9): unresolved ⇒ unproven.

Summary source for imports: a checked-in allowlist table of stdlib
functions (`strings.*` → Pure, `net.Conn.Read` → Blocks, ...) generated
once and versioned with the pinned Go toolchain. Anything absent is
`Unknown`. This is the main maintenance cost (see §6, risk R2).

## 3. Admission and lowering rules (preregistered test 2)

Notation: `Γ` = region context carrying signal `done` (a `<-chan struct{}`).

| Construct | Admission rule | Lowering to ordinary Go |
|---|---|---|
| `task f(args)` in region | args pass existing E3001/E3003 checks; `eff(f) ∈ {Pure,Obs}`; the child's handle is **not** nameable outside the region (no escape) | `sup.go(func(){ f(args) })` — supervisor increments a `WaitGroup`, recovers panics into the region error |
| closure spawn | same, with the closure body summarized in place | same |
| region exit | implicit join; must be the only exit edge for children (break/return/goto leave via the join) | `defer sup.wait()` ordered **before** user defers of region-owned resources |
| `x <- v` | admissible only if `done` is reachable in `Γ` | `select { case x <- v: ; case <-done: return ErrCancelled }` |
| `<-x` / `v, ok := <-x` | same | `select { case v, ok = <-x: ; case <-done: return }` |
| user `select` | must contain a cancel case or is wrapped; `default`-only is `Pure` | add `case <-done:` arm |
| `for { }` / `for cond {}` / backedge of non-bounded loops | needs a check unless the body contains an `Obs` blocking op on every path | insert `if done.err() { return }` at the backedge (poll is a single atomic load; cost to be measured) |
| bounded `for range n` / slices | no check needed; but body cost is *not* bounded — a documented hole (§6, R3) | none |
| call `g(...)` | `eff(g) ∈ {Pure,Obs}`; callee receives `Γ` implicitly (hidden param) | extra parameter `done` threaded through |
| `managed`/foreign/Rust call | `Unknown` ⇒ not certifiable **inside** the region; allowed outside, or via an explicit `uninterruptible { }` that downgrades the region's claim to "cancel reaches it, bounded wait *not* certified" and is logged in the cert | untouched |
| `defer` / cleanup | cleanup of region-owned resources runs after join (P4); a child holding a resource must be a region child, else E-rule below | ordering by construction of `sup.wait()` placement |
| mutex acquire | treated as `Blocks` unless it is the try-lock/`select` wrapper form | — |

New diagnostic classes (names provisional, would be allocated in the E3xxx
range after the corpus lands): `REGION-E1` unresolved/uninterruptible callee;
`REGION-E2` child escapes (handle or captured region resource stored
outside); `REGION-E3` unguarded blocking op; `REGION-E4` unchecked unbounded
loop; `REGION-E5` region resource used by a non-child.

**Soundness sketch.** P3 is by construction (single join point). P4 follows
from P3 plus the escape rule (no non-child can hold a region resource). P1
holds because `done` is only threaded as a hidden parameter, so a callee
cannot be summarized `Obs` without receiving it. P2 is the crux: it reduces
to *"every blocking effect is summarized"*, so soundness is exactly as good as
the summary table (§2) and the loop-bound analysis. This is **not** a
Tier-0 corollary (contrast W-006): sendability is flow-insensitive over
values; P2 needs an interprocedural effect fixpoint.

## 4. Interaction with existing guarantees

- Rows 12/16: row 12 can flip to a *scoped* claim ("inside an admitted
  region, P holds") — never a global one. Row 16 (send-on-closed) stays
  "no claim" unless region-owned channels additionally get a close-after-join
  rule; P4 gives it for region-owned channels only.
- Sendability (E3001/E3002) still runs unchanged on every spawn/send.
- The two check-sites invariant (C5) is **not** preserved: this adds new
  check sites (blocking ops, loop backedges, calls). That is a real spec
  change and needs a logged decision.
- C2 spawn budget (≤2× Go spawn): `WaitGroup` + `recover` supervisor adds
  allocation; unmeasured. Must be spiked before acceptance.

## 5. Preregistered tests of the idea (falsifiable)

Hypothesis H: *a summary-based admission rule certifies P for a useful
subset at tolerable cost.* H is **refuted** if any of:

1. **Negative suite (must reject, 100%)**: for each of the 5 REGION-E classes
   ≥4 minimal programs (e.g. plain `<-ch` in a child, spin loop, handle
   stored in a global, `managed` call in region, interface-method callee).
   Any silently admitted ⇒ unsound ⇒ H refuted.
2. **Dynamic oracle**: lowered Go for admitted positive programs run under
   `-race` with a cancel injected at *every* scheduling point (a
   `runtime.Gosched` fault injector at each `done` check); assert zero
   leaked goroutines (`goleak`-style) and zero post-join access. Any leak
   ⇒ H refuted.
3. **Corpus yield**: of the ten `examples/` programs plus the 52-program
   corpus, count how many have a region-shaped fragment admissible without
   `uninterruptible`. If < 30 % of fragments admit, the subset is too small
   to be a feature (fails *usefulness*, not soundness).
4. **Cost**: spawn overhead ≤ 2× (C2) and backedge poll ≤ 5 % on a tight
   loop microbench. Failing either narrows to opt-in checks.
5. **Summary-table maintenance**: stdlib coverage needed for the corpus;
   count of `Unknown` hits. If most real programs hit `Unknown` in their
   first blocking call (`net`, `os`, `database/sql`), the certificate is
   vacuous in practice.

## 6. Risks and honest limits

- **R1** Fairness: Go's scheduler gives no formal fairness; P is conditional.
- **R2** Summary table is a Go-API database — the exact thing W-016 (#327)
  argues the project should *avoid* depending on. Direct tension; any
  verdict on W-007 should be read together with W-016.
- **R3** Bounded loops with unbounded bodies; pure compute without checks
  can still delay cancellation arbitrarily long (liveness, not safety).
  Mitigation only by backedge checks everywhere, at a cost.
- **R4** Hidden `done` parameter changes function ABI: calls from outside a
  region to the same function need a second (non-cancellable) instantiation
  — code growth, duplicated emit paths.
- **R5** Rust/`managed` code cannot observe `done`; the region claim ends at
  that boundary by design (Prime Directive 6).
- **R6** Overlap: W-006 (#352, dataflow corollary) and W-008 (#257,
  settlement accounting) pursue adjacent claims; they should be reviewed as
  one family and a common supervisor runtime shared.

## 7. Cheapest first spike (no compiler)

Hand-lower three programs to Go (producer/consumer pipeline, fan-out/fan-in
with a nested callee, a worker pool with cleanup), implement the supervisor
in `runtime/go` (≈60 lines), and run test 2 plus test 4. Pass/fail of those
two numbers decides whether any compiler work is justified. A summary
analyzer prototype over `go/ast` for test 5 can follow independently.

## 8. Provisional verdict

**CONDITIONAL.** The soundness argument is plausible *because* it reduces
cancellation safety to a fail-closed effect summary, which matches existing
checker posture; it is neither free nor a Tier-0 corollary. It lives or dies
on tests 3 and 5 (usefulness) and on resolving R2 vs W-016. Recommend
accepting as a scoped, opt-in feature track **only if** spike §7 passes and a
logged decision relaxes C5's "exactly two check-sites".
