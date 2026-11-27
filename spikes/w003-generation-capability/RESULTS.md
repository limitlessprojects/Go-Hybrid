# Spike W-003 v2 — per-storage generation capability vs. the family's P4 residual

Ledger row: **W-003** (#193) · capabilities family (W-001 / W-002 / W-003 / W-014 / W-017).
Supersedes the closed queue-swept PR #1071 (closed 2026-10-24 by the
queue-reset sweep, "not a verdict on the work"; it never reached the
2026-11-01 window's input set). Scope: `spikes/w003-generation-capability/`
only — no compiler, runtime, or guarded files touched.

## Why this spike exists

The 2026-11-01 window re-slotted the capabilities family because its
artifacts overlap, and three independent spikes converged on one residual:
**P4 — a closure-captured or missed alias keeps using storage after `move`**
(#370 `TestNeg_FastPathMissedAlias` / `TestNeg_DeferArgEvaluatedBeforeMove`,
#507 `TestP4ClosureCaptureIsTheDeclaredFalsifier`, #545 P4a). All three
mechanisms act on the *binding* (a witness, a zeroed variable, a freeze
fact); the alias holds the *storage*. W-003's distinguishing claim is a
runtime mark on the storage itself. This spike tests that claim directly on
the family's own falsifier shapes, and tests the proposal's own kill
criterion 1 (overhead).

## Pre-registration (committed before any code in this directory ran)

Mechanism under test (**G**): `Own[T]` = `{h *header[T], gen uint32}`;
`header[T]` = `{gen atomic.Uint32, v T}`. Every syntactic use of an
`own[T]` binding lowers to `x.Get()` (one atomic load + compare against the
capability's own generation, fail closed on mismatch). `move x` lowers to
`x.Move()` (CAS `gen → gen+1`, returns the only live capability). A task
spawn of a closure lowers to `Move()` of each captured capability (the
free-variable list is syntactic, not alias reconstruction). Variant **B1**
is the literal "one bit" of #193: the same code with the generation taken
mod 2.

Shapes (S1–S3 are the family's published falsifiers, reproduced verbatim in
spirit; S4–S5 are new):

| Shape | Source | Program |
|---|---|---|
| S1 | #507 P4 | closure captures `x`; `x` moved into a task; closure runs and writes |
| S2 | #370 N0 | `alias := m` missed by may_share; `m` moved into a task; `alias` writes |
| S3 | #370 E1 | `defer f(m)` evaluated before `move m`; task writes; defer writes |
| S4 | new | stale alias survives two moves (A→B→C) and writes (ABA) |
| S5 | new | a raw `T` extracted with `Get()` is kept in a plain-Go variable across the move |

Predictions and falsifiers:

- **H1 (S1–S3 fail closed under G).** Each stale use panics with
  `hyb: use of moved value` before touching storage, in 100/100 iterations,
  and `go test -race` reports **0** races. *Falsified* if any stale write
  completes or the race detector fires on a G variant.
- **H2 (controls race).** The same S1–S3 shapes under M0 (today's identity
  `move`) and under the W-017 fast-path take (`__hy_take`, as in #370)
  trigger `WARNING: DATA RACE` deterministically. *Falsified* if a control
  is race-free (that would mean the harness cannot see the bug).
- **H3 (one bit is not enough — predicted to FALSIFY B1).** Under B1, S4's
  stale alias passes the check after two moves; under G it fails closed.
  If confirmed, #193's "one bit" claim must be restated as "one generation
  word" (still one word, still zero allocations per move).
- **H4 (kill criterion 1 of #193: median overhead ≤ 5%).** Per-access check
  on map-heavy and slice-heavy single-task loops, `-count=10`, median vs.
  plain Go. Predicted: map-heavy ≤ 5%; tight slice-index loops > 5% per
  access, ≤ 5% when the emitter hoists one `Get()` per straight-line region
  with no intervening `move`/call. Whatever the numbers are, they are
  reported, and a > 5% hoisted result kills the runtime half for that
  workload class.
- **H5 (declared limit, predicted to race).** S5 races under G: a raw `T`
  that escaped into ambient Go is invisible to any storage mark that is not
  checked by ambient code. This is the residual a static rule must own
  (`view[T]`/raw values not storable, #193 sketch §1) — declared here, not
  discovered later.

## Results

*(filled in by the results commit; nothing above this line changes after
the first run)*
