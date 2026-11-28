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

Host: Go 1.24.7 linux/amd64, Intel Xeon @ 2.10 GHz, 2 vCPU shared VM (the
pinned go1.26.9 could not be downloaded in this sandbox; nothing here uses a
1.25+ feature, `go.mod` says `go 1.24`). Reproduce:

```bash
cd spikes/w003-generation-capability/work
CGO_ENABLED=1 CC=gcc go test -race -count=5 -v .   # H1, H3 (G and B1 rows)
./run_negatives.sh                                 # H2 controls + H5, each 3x
go test -run '^$' -bench . -benchmem -count=20 .   # H4 wall clock
```

### Verdicts

| | Prediction | Outcome |
|---|---|---|
| H1 | S1–S3 fail closed under G, 0 races | **Held.** 100/100 iterations per shape under `-race -count=5`: every stale use panicked `hyb: use of moved value` before touching storage, and the live capability saw only its own writes. 0 races. |
| H2 | M0 and W-017 take race on S1–S3 | **Held where the shape races.** `DATA RACE` in 3/3 runs for M0 S1, M0 S2, M0 S3, take S2, and take S3. Take on S1 does not race, because Go closures capture the *variable*. A captured write after `__hy_take` hits Go's `assignment to entry in nil map` panic. A captured read silently returns the zero value (`TestTake_S1_CapturedReadIsSilentZero`). Neither is a race or a hyb diagnostic, and the read path gives a wrong answer. |
| H3 | One bit is not enough | **Held: B1 falsified.** After two moves the mod-2 mark matches again, and the stale write reaches storage (`TestB1_S4_…`). G rejects the same program. **Correction to #193:** "one bit per header" must read **"one generation word per header"**. The cost class is unchanged: one load per use, one CAS per move, zero allocations. |
| H4 | Kill criterion 1 (≤ 5%) | **Split.** It holds for map-heavy code. Per-access checks on tight slice loops fail it outright. Hoisted checks pass by construction, but this host cannot measure below its ±20% noise. Details below. |
| H5 | Raw escape races (declared limit) | **Held.** `TestNeg_G_S5_…` races in 3/3 runs. A raw `T` kept in an ambient-Go variable is invisible to the mark. It needs the static "raw/`view[T]` not storable" rule from #193's sketch. |

### H4 numbers

Wall-clock time is noisy on this shared VM: min–max spreads reach ±20–30% within one
benchmark. Instruction counts from `valgrind --tool=callgrind` are therefore the
primary evidence, and wall-clock medians (20 runs) are secondary.

| Workload | Instructions vs plain | Instructions vs unchecked wrapper | Wall-clock median |
|---|---|---|---|
| Map-heavy, check on every access | **+0.07%** (362.05M vs 361.81M, N=2M) | +1.7% | plain 16.9 ns · per-access 15.7 ns (noise) |
| Slice index loop, check on every access | **+99%** (9.91M vs 4.97M, 200 × 4096) | +50% | +85% to +95% |
| Slice index loop, one hoisted `Get()` per op | 1 load + 1 compare per 4096 elements (≈ 0.02% by construction) | — | same-kernel pair: −9.6% (noise) |
| `Move()` (per transfer, not per use) | — | — | **7.0–7.8 ns, 0 B/op, 0 allocs/op** |

The check costs little next to a map operation, so map-heavy
paths stay inside the 5% budget even when every access is checked.

A tight slice loop cannot afford a check per element. The emitter must hoist one
`Get()` per straight-line region with no intervening `move` or call, which is the
same region the existing checker already reasons about.

The first slice benchmark pair used separate loops and showed +36% wall-clock time.
Yet `go tool objdump` shows the inner loops are instruction-identical, apart from an
alignment `NOP` the Go compiler placed inside the hoisted loop. The same-kernel pair
removes that layout effect and shows no measurable difference.

This prototype also pays a wrapper indirection, because `Mint` allocates a header.
The unchecked wrapper alone measured +13–20% wall-clock time on maps. That cost comes
from prototyping in user-space Go, not from the check. The header-embedded variant needs
runtime/emitter support and is **not** measured here.

PR #1071 and the issue #193 pre-review claimed the mark fits in existing `hmap` padding
at zero bytes. That claim is **withdrawn as unverified**.

### What this changes for the family

| Shape | M0 (today) | W-017 take (#370) | W-004 witnesses (#507) | W-003 G (this spike) |
|---|---|---|---|---|
| S1 captured use after move | race | write: Go nil-map panic · read: silent zero | race (their P4) | **fails closed** |
| S2 missed alias | race | race (their N0) | race | **fails closed** |
| S3 defer arg before move | race | race (their E1) | race | **fails closed** |
| S4 stale after two moves | silent | silent | compile error if not captured | **fails closed** (B1: silent) |
| S5 raw escape to ambient Go | race | race | race | race (**static rule required**) |
| Cost per use / per move | 0 / 0 | 0 / 0.3 ns | 0 / 0 | 1 load / 7 ns CAS |

The witness and take designs act on the binding, so any second binding
(closure, alias, defer) escapes them. That is the P4 convergence the window
recorded. A mark on the storage does not depend on how many bindings exist: every
stale binding fails on its next use.

What remains for static machinery is
smaller and local: a raw or `view` value may not be stored or captured (S5).

W-003 makes exactly this claim, and it now has a race-verified harness behind it. The
spike also makes two corrections: a generation word, not one bit, and the header
padding claim is withdrawn.

### Limitations

- L1: this is a user-space prototype with a wrapper header per mint, not the
  header-embedded layout.
- L2: S5 (raw/`view` escape) is not covered by the runtime mark; it needs the
  static rule.
- L3: there is no compiler or emitter change. The lowering is written by hand in
  the tests: `Get()` per use, `Move()` per move, and `Move()` of captured
  capabilities at a task spawn.
- L4: Go 1.24.7 instead of the pinned 1.26.9, on a single shared-VM host.
