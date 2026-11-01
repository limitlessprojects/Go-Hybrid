# W-005 spike: message isolation — clone mutable object graphs at task and
# channel boundaries

**Status:** spike complete 2026-10-17 (no-compiler artifact, per the issue's
own first-milestone directive: "The first milestone should be a no-compiler
spike, not a rewrite"). Code: `work/` — Go module `hybrid.dev/spikes/w005`,
package `iso` + four runnable demos. **Run:** `cd work && go test ./... -race`
and `go run ./demos/r1-task-isolation` etc.
**Source idea:** wildcard W-005, nikolas-sapa, issue #342 (dup-credit #308).
**Question (the issue's test plan, step 1–3):** can a memoized type-directed
deep clone give task/channel boundaries actor-style message isolation — no
shared mutable GC graph across tasks — while preserving alias/cycle
relationships INSIDE each side's graph, failing closed on uncloneable values,
and what does it cost against the 2× spawn budget?

## Design under test

`work/clone.go` implements the bounded cloneability contract by reflection
(the shim for what the compiler would generate at task/send sites):

1. **Deep clone with internal identity.** The memo table keys the REFERENCED
   object — `(type, pointer)` for maps/pointers/chans, `(type, pointer, len,
   cap)` for slices so distinct views of one backing array stay distinct
   (identical headers unify). Two names aliasing one map at the boundary
   alias the copy; cycles come back as cycles; pointer-keyed maps keep one
   entry per distinct key.
2. **Channels copy by identity** — clone the message, never the pipe.
3. **Scalars copy by value** — immutable, sharing unobservable.
4. **FAIL-CLOSED rejections** (no silent fallback to shared references):
   func values, uintptr/unsafe pointers, and structs with unexported fields
   (reflection cannot set them without unsafe; the real integration would
   generate clone code inside the defining package).
5. **Structs/arrays recurse without memo** — they have no Go identity; any
   aliasing through them goes via pointers, which the memo covers.

`work/isolate.go` emulates the generated boundary code: `Spawn[T](fn, arg)`
clones arg, then runs fn on the copy in a fresh goroutine (the caller keeps
its original); `Send[T](ch, v)` clones v and delivers the copy. Spike
contract: the callee reaches mutable state only through its parameters —
under compiler integration the checker would enforce that statically.

## Pre-registered predictions (written before the demo runs)

- **P1 (race-freedom):** the #229 primary repro re-encoded through
  `iso.Spawn` — worker and sender both hammering their (distinct) maps —
  runs under `go run -race` with ZERO reports. (Under CURRENT Hybrid rules
  the same program is E3001-rejected; on pre-#230 main its emitted Go
  produced `WARNING: DATA RACE` — concurrent map writes — recorded in the
  pr-230 merge evidence.)
- **P2 (identity preservation):** cycles survive (walk N steps, return to
  start); internal aliasing survives (write via A, read via B); pointer-keyed
  maps do not duplicate aliased keys.
- **P3 (chan identity):** a struct holding a chan clones the struct's
  mutable fields but the chan field still IS the original pipe.
- **P4 (fail-closed):** func values / unexported fields reject with errors
  at the boundary — never a silent shared reference.
- **P5 (budget — the issue's own pre-registered failure path):** a realistic
  10k-node cyclic graph clones in ~100× a spawn. Prediction: the 2× spawn
  budget FAILS by orders of magnitude for large graphs; the model can only
  fit bounded message sizes.

## Results (all measured 2026-10-17, Go 1.26.9, this sandbox)

| Claim | Result |
| --- | --- |
| P1 race-freedom under isolation | **PASS** — `go test ./... -race` ok (7/7); R1 demo: zero reports, sender ends with 2 own keys, `worker` key invisible |
| P2 cycles preserved | **PASS** — 2-cycle walked 6 steps returns to start; copy ≠ original |
| P2 internal aliasing preserved | **PASS** — write via A visible via B inside the copy; sender map untouched |
| P2 pointer-keyed map | **PASS** — aliased key stays one entry; `g.Keys[g.Root]` resolves |
| P3 chan identity | **PASS** — `received.Ctl == ctl` in the source, 42 crosses the same pipe |
| P4 fail-closed | **PASS** — func value, func-holding struct, unexported-field struct all REJECTED with precise errors at Spawn/Clone |
| P5 budget (2× spawn) | **FAIL as predicted** — spawn ≈ 2.5µs; clone of 10k-node cyclic graph ≈ 100ms ⇒ ≈ 40,000× budget |

Demos (`work/demos/`, all output as documented inline):
`r1-task-isolation` (the #229 repro, isolated), `r2-alias-inside-copy`
(alias + cycle through the boundary), `r3-chan-identity`, `r4-fail-closed`.

## Verdict for the M1 window (2026-11-01, discussion #365)

**CONDITIONAL — do not adopt blanket clone-at-boundary as the default
task/send semantics; the correctness half of the idea is proven, the cost
half fails its own pre-registered budget.**

- **Proven:** message isolation is implementable exactly as specified — the
  memoized clone preserves precisely the identities that must be preserved
  (cycles, internal aliases, pointer keys) and destroys precisely the ones
  that must be destroyed (boundary-crossing identity). Fail-closed rejections
  are honest and surface at the boundary. This is the hard 20% of the idea.
- **Failed:** the 2× spawn budget at realistic graph sizes (~40,000× over).
  Per the issue's own pre-registration: "if realistic graphs exceed it,
  publish the failure and keep the current model."
- **Viable narrowings for a follow-up milestone:** (a) opt-in isolation
  (an `isolated` capability/type checked at the boundary — clone only where
  the programmer grants it); (b) bounded-actor mailboxes (clone only small
  message structs; large state stays behind handles); (c) copy-on-write /
  structural sharing to cut clone cost. Any of these keeps the proven
  memo-clone core and sidesteps the blanket-cost failure.
- **Not measured (needs compiler integration, deferred):** the ≤20%
  annotation-rate criterion on the idiomatic positive corpus — under
  isolation semantics, today's E3001 map-at-task rejects become legal, so
  the annotation rate is expected to DROP, but only a real emit-side probe
  can count it.

## What broke / limitations (honest ledger)

1. **Unexported fields reject** — the spike cannot clone them without
   `unsafe`; stdlib-embedded structs (e.g. `time.Time`) therefore reject at
   the boundary. The compiler-integrated version generates clones
   in-package and would not have this limit, but the spike surfaces the
   real semantic decision: unexported state needs an explicit story.
2. **Distinct slice views of one array are cloned independently** —
   same-header views unify (memo), but `a[0:2]` and `a[1:5]` of one array
   become independent copies; writes through one no longer appear through
   the other INSIDE the copy. Compiler-generated code can preserve view
   aliasing by cloning at the backing-array level; the reflection shim
   cannot see the sharing without that. Documented divergence, not
   corruption.
3. **Callee discipline is unenforced here** — `Spawn`'s fn must reach
   mutable state only via parameters; the checker half of the integration
   (captures become clone candidates) is future work.
4. **Reflection cost dominates** — ~100ms for 10k nodes is reflection-based
   and unoptimized; generated type-directed clone code would be several ×
   faster, but still thousands× a spawn at this size. The budget failure is
   structural (O(graph size) per boundary crossing), not an artifact of the
   shim.
