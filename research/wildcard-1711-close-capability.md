# Wildcard #1711 — Close-right confinement: the send-on-closed half is reachable; the channel-DAG half is false

**Issue:** [#1711](https://github.com/limitlessprojects/Go-Hybrid/issues/1711) · **Commit examined:** `1f0ff97` · **Status:** design-only; no compiler changes in this PR.

## 0. Summary

#1711 proposes affine single-sender/single-receiver endpoints with
`close(move tx)`, plus a static channel-topology DAG that "eliminates
circular-wait deadlocks by construction." The first is close to
implementable. The second is false in both directions and rejects the repo's
own pre-registered positive control (corpus 16).

The salvageable subset is smaller: **you do not need affine endpoints to kill
send-on-closed panics; you need to make the close right linear.** A channel
may have many senders and still be safe if none can close it; a channel with
one sender is unsafe if a second task holds the close right. Tracking
close-right confinement flips `GUARANTEES.md` row 16 with zero runtime cost
and zero corpus changes, while preserving MPSC fan-in (corpus 15) and
request/ack pipelines (corpus 16). Row 12 stays **no claim**; the DAG does
not earn it.

## 1. The DAG claim is not sufficient

A blocking send waits for its receiver, so its wait edge runs *opposite* the
data edge; a blocking receive waits for its sender. A wait-for cycle is
therefore not a channel-graph cycle. This program is accepted today with 0
diagnostics:

```hy
fn two(out chan<- int) {
    out <- 1
    out <- 2
}

fn three(to_two chan<- int, to_four chan<- int) {
    to_two <- 1
    to_four <- 1
}

fn four(from_three <-chan int, from_two <-chan int, done chan<- bool) {
    <-from_three
    <-from_two
    done <- true
}
fn main() {
    t24 := make(chan int, 1)
    t32 := make(chan int)   // unbuffered
    t34 := make(chan int)   // unbuffered
    done := make(chan bool, 1)
    task four(t34, t24, done)
    task three(t32, t34)
    task two(t24)
    <-done
    fmt.println("finished")
}
```

Channel edges: `three → two` (`t32`), `three → four` (`t34`),
`two → four` (`t24`), `four → main` (`done`). The graph is acyclic and every
channel has a unique sender and receiver. It still deadlocks: `two` blocks on
its second send to the full `t24`; `four` blocks on `t34`; `three` blocks
sending to `t32` because `two` never receives. The emitted Go reproduces
Go's detector: `fatal error: all goroutines are asleep - deadlock!`

Change only the buffers — `t24` to 2, `t32` to 1, `t34` to 1 — and the graph
is identical but the emitted Go prints `finished`. **Deadlock freedom is a
property of reachable blocking states (buffer occupancy, select readiness),
not of the topology.** No topology-only predicate can be both sound and
precise.

## 2. The DAG claim is not necessary — and it rejects corpus 16

`tests/checker-corpus/16-move-send-clean.hy` is pre-registered `ACCEPT`:
main sends eight batches, consumer receives and sends eight acks, main drains
eight acks, both buffers are 8. `hyb check` returns `OK (0 diagnostics)` and
the program terminates. Its channel graph is a two-cycle —
`main → consumer` on `batches`, `consumer → main` on `acks`. A literal DAG
rule rejects this unless replies are special-cased, at which point it is no
longer a DAG and needs the protocol analysis it was trying to avoid.

Naive wait-for acyclicity does not rescue it: while main sends it does not
receive, and while consumer sends acks it does not receive batches. The union
wait-for graph is cyclic, but the buffers make the waits non-simultaneous.
This is the point W-009's triage circles: "wait-for epochs" must model
capacity and phase, not order. The `split(k)` and function-return questions
are downstream; the topology premise is the load-bearing error.

**Consequence:** withdraw the deadlock half of #1711. A row-12 flip needs a
closed protocol subset (W-010) or buffer-certified networks (W-018), not an
acyclic channel graph.

## 3. The salvageable half: close-right confinement

`send on closed channel` needs a send *after* a close; `close of closed
channel` needs two closes. What must be linear is the **close right**, not
the send right. This distinction preserves exactly the patterns affine
endpoints break.

### 3.1 Rules

- **S1 (identity).** Every `make(chan T, n)` gets a `CapId`. Aliases,
  assignments, and `move` share it; rebinding a name to a different channel
  creates a fresh id.
- **S2 (sharing).** Any operation that can make the send capability reachable
  from another task marks the id **shared**: a spawn/send/return/foreign-call
  argument, payload, or composite that contains a send-capable channel and
  crosses a task boundary; a spawn/call parameter whose channel direction is
  `chan T` or `chan<- T`; capturing it in a spawned or stored closure; or
  passing it through `any`/an unresolved shape (fail closed). A closed
  send-cap may cross only as `<-chan T`; otherwise reject.
- **S3 (close).** `close(x)` is accepted only when `x`'s id is send-capable,
  unshared, and not already closed. It consumes the **send capability** of
  `x` and every alias. A later send or close through any alias is
  `HYB-E3003` (use-after-close); a close after sharing or of a non-owned
  channel is new `HYB-E3013` (next free E30xx). A channel whose id is opaque
  — a parameter, a receive result, a return, or a foreign call — is
  non-closeable until a freshness/ownership summary says otherwise. Branch
  joins take the may-closed state: a close in any `if`/`select`/`match` arm
  makes every later send or close reject. Receives, `for x in ch`, `len`, and
  `cap` remain legal.
- **S4 (defer/loops).** `defer close(x)` installs a pending close obligation:
  sends remain legal until return, a second close (including a second
  deferred one) rejects, and a loop-carried close — deferred or not —
  follows `check_loop_backedge` (a channel made inside the loop is a fresh id
  per iteration; a loop-invariant one rejects). Deferred LIFO ordering with
  other deferred calls that send keeps those shapes outside the claim unless
  the ordering is statically trivial.
- **S5 (hatch).** A shared-sender channel that must be closed uses
  `managed("reason", "actor")` and leaves the guarantee, as other `managed`
  values do.

### 3.2 Why this is better than affine `tx`/`rx`

| Pattern | #1711 affine endpoints | close-right confinement |
|---|---|---|
| MPSC fan-in (corpus 15: four workers share `results chan<-`) | needs `split(k)` or rewrite | unchanged; channel is simply not closeable |
| Request/ack (corpus 16) | channel graph is a cycle; DAG rejects | unchanged; `batches` closeable, `acks` not |
| Post-close drain (`for x in ch`) | `close(move tx)` rejects the drain (observed `HYB-E3003`) | legal; only the send cap is consumed |
| `.hy` migration | every bidirectional channel crossing a boundary changes | no change at accepted corpus close sites (16, 18, member-defer) |
| Runtime | zero | zero |

### 3.3 The machinery already exists

Observed at `1f0ff97`:

- `close(move ch); ch <- 1` → `HYB-E3003` from the existing O5 move pass.
- `close(move ch); for x in ch {}` → also `HYB-E3003`: the current move
  consumes the whole binding, including the receive side. That is why
  #1711's `close(move sender)` needs a split endpoint, and why send-cap-only
  state is the smaller fix.
- `close(move ch)` emits plain `close(ch)` — `hy-emit/src/expr.rs:162`
  strips the move marker.
- `close(ch); ch <- 1` and two closes through an alias are both accepted
  today and panic in the emitted Go (`panic: send on closed channel`,
  `panic: close of closed channel`). These are the repros the rule must
  reject.

### 3.4 Implementation sketch

- `model.rs:139` `Binding` gains `chan_cap: Option<CapId>`;
  `FnState` (`model.rs:217`) gains `caps: HashMap<CapId, CapState>` with
  `shared`, `closed`, and `pending_defer` spans. Reuse `define_binding`
  (`loans.rs:5080`) to intern/copy the id from an initializer.
- `sendability.rs:834` `check_send_target` is the single path for statement
  sends and `select` send arms; add the closed-cap check (E3003) there.
  `check_call_arg_chan_dirs` (`:1948`) already resolves each spawn/call
  parameter's direction, so S2 piggybacks on its result.
- `sendability.rs:1460` `check_fn_close_and_range_directions` already walks
  every `close`, including `defer` and closure bodies, and enforces
  `HYB-E3009` for `<-chan`. Add the cap-state check and consume there; do
  not route close through `mark_move`, which freezes the receive side.
- `freeze.rs:298` `mark_move` stays for ordinary moves; a `close_send_cap`
  helper propagates closed state across the alias group. `freeze.rs:608`
  `check_loop_backedge` clones `FnState`, so cap state rides along.
- `hy-diag/data/diagnostics.json`: register E3013; reuse E3003 with a
  channel-specific note. Emitter and runtime: no change.

Estimated cost: a few hundred lines plus fixtures, dominated by escape sites
and negative tests — no new pass, no runtime allocation.

## 4. Guarantee, breaks, migration

**Proposed row 16.** For channels allocated by `make` in the checked subset
and never handed to `managed`/foreign code, the checker guarantees no
`send on closed channel`, no `close of closed channel`, and no close
concurrent with a send from a checked task. The close site is the audit
point; the diagnostic names the escape site.

**Breaks.** (a) Closing an MPSC channel after handing its send end to workers
rejects E3013; the channel is still usable for sends, just not closeable.
A `split(k)`/join-token extension (W-009's territory) could recover the
supervised close later; it is orthogonal to row 16. (b) Closing a channel
parameter, a returned channel, a received channel, or a channel reached
through an unresolved composite rejects until a freshness/ownership summary
exists. (c) `close(nil)` still panics; the claim is scoped to `make`-created
channels, not the N3 zero channel. (d) Conservative escape marking through
`any`/foreign calls may reject benign local uses; `managed` is the valve.

**Migration.** `close(ch)` becomes consuming automatically, so accepted
corpus close sites need no edits: corpus 16 closes `batches` after passing
only its `<-chan` receive end to the consumer; corpus 18 does the same inside
`managed`; the `defer close(ch)` in member-defer closes a local channel.
`close(move ch)` stays as the explicit spelling (and already emits
`close(ch)`). Shared-sender close sites are the only ones needing
restructuring: keep the send end in the closing task (single-producer) or
use `managed`. No blanket `chan T` rewrite, no signature change.

## 5. Falsifiers

1. **Panic freedom.** Adversarial alias, field/index, `select`, `defer`,
   rebinding, composite, closure, and `any` shapes must reject or never
   panic. Run accepted programs under `-race -count=1000` with delayed sends;
   any accepted `send on closed channel`/`close of closed channel` falsifies
   the claim.
2. **No over-rejection.** Corpus 15/16/18 and the ten pinned examples must
   remain accepted; if not, the cost is too high.
3. **Escape soundness.** Deliberate laundering (`struct{ch chan int}`,
   `[]chan int`, `any`, method receiver, foreign call) must reject E3013 or
   be outside the claim with a marker. A successful launder falsifies it.
4. **Emitter parity.** Every accepted close lowers to `close(x)` and builds;
   no hidden runtime check.
5. **DAG falsification (already fired).** The diamond and corpus 16 are the
   counterexamples: any topology rule must either admit corpus 16 and reject
   the diamond, or explain why it knowingly breaks a pre-registered ACCEPT.

## 6. Honest risks

- **The escape analysis is the whole soundness argument.** Binding-level
  tracking is not enough: a send-capable channel inside `any`, a composite,
  or a foreign call can be closed elsewhere. Fail closed on unknown shapes,
  and exclude those values explicitly. If the exclusion is too broad, the
  usable subset shrinks and row 16 becomes marketing; measure on the corpus
  before publishing.
- **No interprocedural summary exists today** (`lib.rs:1695` notes the
  absence). Returned/parameter channels are conservatively non-closeable;
  adding a freshness summary is future work, not M1.
- **`defer`/multiple returns/panic paths.** A missed discharge path must
  reject, and the loop back-edge must carry cap state or a defer-in-loop
  double-close slips through.
- **Liveness remains unproven.** A shared-sender channel that is never
  closed leaks receivers; so does a closeable one. Row 12 must stay no claim.
  This buys one row, not two.
- **The negative result is the main risk to #1711's framing:** the topology
  premise cannot carry "complete concurrency verification." The honest
  salvage is a smaller row-16 guarantee plus W-010/W-018 for progress.

## 7. Track terms

Design-only; no prototype required. Falsifying the DAG half is part of the
deliverable. If accepted, M1 is the row-16 text plus negative fixtures in
`tests/checker-corpus/`; M2 is the capability pass, differential run, and the
published learning that topology-only deadlock rules are neither necessary
nor sufficient.
