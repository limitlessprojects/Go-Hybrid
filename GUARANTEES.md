# GUARANTEES.md — the claims table

**What exactly does this compiler guarantee about my code?** You asked
([Discussion #1](https://github.com/limitlessprojects/Go-Hybrid/discussions/1)),
here is the line-by-line answer. This table is generated from the
specification, not the other way around: every row cites the SPEC rule that
makes the claim and the evidence that tests it. Where we have no claim, the
row says **no claim** — a blank hedge is the thing this document exists to
prevent.

The normative source is [SPEC-LANGUAGE.md §9.2](SPEC-LANGUAGE.md) (binding,
quote-shape — marketing may quote it, never weaken it):

> **The safety claim, stated once, in full.** Hybrid's compiler guarantees
> freedom from data races **only for Hybrid-checked sites in the checked
> subset**: every value that crosses a `task` spawn or a channel send is
> verified sendable by the Tier-0 rules of §9, and every loan inside a
> function body is verified conflict-free. Values inside `managed` blocks
> are **outside this guarantee** — there the contract is Go's data-race-free
> sequential-consistency discipline, with `-race` instrumentation as the
> backstop. Values escaping into foreign Go code (net/http handlers, Go-API
> callbacks) are likewise outside it: **Go-API escapes follow Go's DRF-SC
> memory model with `go test -race` as the backstop.** Memory safety on the
> Go side is Go's GC; on the Rust side it is single ownership, moves, and
> function-local borrows; at the boundary it is owning-side handle tables
> with generation tags. No claim is made about `unsafe` code, about foreign
> code the checker cannot see, or about any program the checker accepts in
> error — an accepted unsound program is a P0 bug (SPEC §10), and this
> claim is exactly as strong as the checker and its adversarial corpus
> prove it to be.

## The claim in one line

Everything the checker **accepts was proven** sendable; everything it
**cannot prove is named** with a diagnostic that teaches the fix
([O4.9](SPEC-LANGUAGE.md), rule FC). There is no permissive unknown.

## The claims table

<!-- BEGIN GENERATED CLAIMS TABLE (tools/check_guarantees.py --render) -->
| # | Surface | What the checker does | The claim | Evidence |
|---|---------|----------------------|-----------|----------|
| 1 | `task` spawn arguments | Every argument classified by the six sendable classes; shared mutable aliases rejected | **Guaranteed**: no value reaches a spawned task through an unproven path | [§8.4 C5](SPEC-LANGUAGE.md), [§9.3 O4](SPEC-LANGUAGE.md); corpus [01](tests/checker-corpus/01-shared-map-across-tasks.hy) (reject) / [15](tests/checker-corpus/15-fanin-per-task-state.hy) (accept) |
| 2 | Channel sends (`ch <- v`, `select` send) | Same classification as spawn; failures `HYB-E3002` with a fix-it menu | **Guaranteed**: the sent value is moved, frozen, copied, or knowingly managed | §8.4 C5, §9.3; corpus [16-move-send-clean](tests/checker-corpus/16-move-send-clean.hy), [17-atomic-channel-immutable](tests/checker-corpus/17-atomic-channel-immutable.hy) |
| 3 | Composites (struct/slice/map literals) | **Deep** sendability: every reference-typed field/element must itself be sendable; a fresh literal wrapping a shared map is still rejected | **Guaranteed**: embedding an alias does not launder it | §9.3 O4.6; corpus [03](tests/checker-corpus/03-pool-shared-struct-unguarded.hy) shape; differential suite |
| 4 | Unknown / unresolvable types and names | **Fail-closed**: unproven values are rejected (`HYB-E3005`) — never silently passed as safe. Fix-its: ascribe / route through a channel / take the hatch | **Guaranteed**: the checker never blesses what it cannot resolve | §9.3 O4.9 (rule FC); dedicated checker tests; corpus `34-select-send-unproven-name` |
| 5 | Closure bodies | `task`/send statements inside a closure are real boundaries (goroutines in the emitted Go) and are checked fail-closed, `managed`-escapable | **Guaranteed** for boundary statements; a closure that writes a captured mutable binding and is passed to a foreign Go call is checked too (HYB-E3013, the [A5](SPEC-LANGUAGE.md) fourth check-site); captures without a boundary statement or an escape write remain **outside** | §9.3 O4.10; corpus [10](tests/checker-corpus/10-a5-http-handler-capture.hy) flipped ACCEPT→REJECT by the A5 fourth check-site (W-016 M2/M3) per its own pre-registered OQ1 condition |
| 6 | `move` / freeze | Ownership transfer with use-after-move rejected (`HYB-E3003`) | **Guaranteed**: a moved value cannot be used by the sender afterwards | §9.3 O5; corpus [16-move-send-clean](tests/checker-corpus/16-move-send-clean.hy); differential reject-side |
| 7 | Loans inside a function body | Polonius-style dataflow over borrows/loans; conflict-free proven or rejected | **Guaranteed** within the checked subset | §9.1 rules O1–O3 (E31xx family); checker unit tests |
| 8 | `managed("reason", "category", tickets...) { ... }` regions | Explicit hatch: linted (`HYB-E8001`), payload REQUIRED (bare block rejects `HYB-E0003`; unknown category rejects `HYB-E0004` — closed set: mutex/atomic/actor/finalizer/interop/other), greppable, counted per category (`--report-managed`), optionally forbidden (`--deny-managed`), and CI-gated for NEW blocks (`tools/managed_gate.py` + baseline) | **No soundness claim inside** — it is a compile-time permit, not a synchronization mechanism. The escape is auditable, never silent | §9.5 (row 14, discussion #4); corpus [02](tests/checker-corpus/02-shared-map-managed.hy), [18](tests/checker-corpus/18-pool-shared-struct-managed.hy); `compiler/crates/hyb/tests/deny_managed.rs` |
| 9 | Values escaping into Go APIs (net/http handlers, callbacks) | A closure that writes a captured mutable binding and is passed to a foreign (package-qualified) Go call is a fourth check-site: rejects with HYB-E3013 unless the call site takes `managed("...", "interop")` or the capture is const/non-`may_share`; read-only captures stay outside the checker | **Guaranteed** (HYB-E3013) for captured-write escapes; read-only captures are **outside** — Go's DRF-SC with `-race` as the backstop | §8.5 (A5 promoted to a fourth check-site, W-016 M2/M3); §9.2; corpus [10](tests/checker-corpus/10-a5-http-handler-capture.hy) pins the reject |
| 10 | `sync.Pool` | Not in the curated surface today; a value of a foreign/unmodeled type at a check-site is **unproven → rejected** (fail-closed), never silently allowed | **No claim**; fail-closed rejection instead of a false one | §8.4 C8 pattern; O4.9 |
| 11 | `sync/atomic` (`atomic.Pointer[T]`, …) | **[TBD]** joins the curated surface post-MVP; when it lands, atomic-typed values become sendable-by-type (class 2). Until then: unproven → rejected | **No claim yet**; the growth path is pre-designed, not improvised | §8.4 C8; [atomic pointer fail-closed corpus](tests/checker-corpus/atomic-pointer-fail-closed-reject.hy) |
| 12 | `context.Context` cancellation trees | Not modeled; the value itself is classified like any other at a check-site | **No claim** about cancellation semantics — the claim is about value sharing, not protocol liveness | §9.2 scope |
| 13 | Finalizers (`runtime.SetFinalizer`) | Not in the surface; not modeled | **No claim** | §9.2 "foreign code the checker cannot see" |
| 14 | `unsafe` | **Reserved, not implemented** ([O20](SPEC-LANGUAGE.md)). There is **no memory-unsafe escape at all** in safe mode: raw cross-language pointers are forbidden, period | **No claim** because there is no surface to claim about | §9.9; abi-types.md rules (hazards P7/P8 demonstrated) |
| 15 | cgo | Not part of `.hy`; cgo code in the surrounding Go project is foreign code the checker cannot see | **No claim** | §9.2 |
| 16 | Send-on-closed-channel panics | The checker proves value sendability, not send/close protocol ordering | **No claim** — a liveness/protocol property, out of Tier-0's scope | §8.4 C5 (check-sites are exactly two) |
| 17 | Memory safety, Go side | Go's GC, unchanged — Hybrid emits Go source | **Inherited from Go** | §9.2 |
| 18 | Memory safety, Rust side | Single ownership, moves, function-local borrows; `panic = abort` at the boundary (never unwind across) | **Guaranteed by construction** | §6.3, abi-types.md |
| 19 | Memory safety, at the boundary | Owning-side handle tables with generation tags; copy-small/pin-large borrows | **Guaranteed** by the Spike-3 design + tests | §9.0 three-layer model; interfaces/abi-types.md |
| 20 | The Rust call path | Guard-checked fast path on the caller's stack; pooled guarded deep stacks; blocking callees via the two-tier scheduler with hard thread budgets | **Performance claimed and measured** (3.37–3.44 ns sandbox fast path; worker-tier 6.93 ns recorded honestly as a MISS vs the 5 ns budget) | [gates/G2.md](gates/G2.md); PLAN.md row 3 |
<!-- END GENERATED CLAIMS TABLE -->

## Evidence inventory (what backs the ✅ rows)

- **Adversarial corpus** — [tests/checker-corpus/](tests/checker-corpus/):
  129 programs, each with an `EXPECTED-VERDICT` header. Programs 01–18 were
  **pre-registered before the checker existed** (the checker was built
  against them, not adjusted after); 19–20 were added during integration
  and 21+ after the checker landed, still hand-derived from the rule text
  first. Current census: **67 REJECT / 32 ACCEPT / 2 ACCEPT-MANAGED**,
  pinned to specific diagnostic codes. Verdicts may change only by a logged
  amendment in [VERDICTS.md](tests/checker-corpus/VERDICTS.md).
- **Differential loop** — [tools/differential_loop.py](tools/differential_loop.py):
  accept-side programs must build and run byte-exact (34/34); reject-side
  must be rejected for the pre-registered reason (67/67). Runs in CI on
  every push.
- **Fail-closed tests** — dedicated checker tests (unknown initializer,
  unresolved name, unproven select-recv → `HYB-E3005`; managed escapes →
  `HYB-E8001`, counted) plus parser no-hang regressions. Full suite:
  `cargo test --workspace` — **808 tests, 0 failures** on the
  `main` CI run for `99c25a4` (run `39060436474`).
- **Fuzzing** — [tools/fuzz/hyfuzz.py](tools/fuzz/hyfuzz.py): default
  `--cases 1200` (token salad + corpus mutations + nest bombs,
  deterministic seed): no hangs, no crashes, no internal errors;
  fleet-pinned on GitHub Actions, where the `006-sanitizers-fuzzing` job
  runs a pre-registered 800-case leg.
- **Sanitizers** — ASan over the four bridge packages (`go test -asan`),
  `-race` over the six runtime packages (`go test ./...`): green on the
  Actions fleet vs the release SHA (`jobs/006-sanitizers-fuzzing`).
- **Honest misses on the record** — the worker-tier fast path measured
  6.93 ns against the 5 ns budget: **recorded as a MISS**, not hidden
  ([PLAN.md](PLAN.md) queue row 3).

## Soundness-bug protocol

An accepted-but-unsound program is a **P0** by definition ([SPEC §10](SPEC.md)).
Open an issue with the smallest `.hy` program that demonstrates it; label
it `soundness`. We commit to: publishing the bug with severity and status,
a regression test in the corpus before the fix lands, and the fix note in
the next release's changelog. **There is now a formal bounty behind this
protocol: a $500 pool over the top 10 contributions, paid only on a
reproduced accepted-but-unsound regression** — full rules, severity tiers,
and the Hall of Fame live in [BOUNTY.md](BOUNTY.md). **The program also
carries a $500 Wildcard Track** for revolutionary, contrarian, high-risk
ideas that a normal feature bounty would reject — no prototype required,
breaking changes and rewrites explicitly allowed, failure-with-knowledge a
valid (and paid) outcome, all outcomes published: [MOONSHOT.md](MOONSHOT.md).

## Known limits of this document

- The table covers **Tier-0 (sendability/ownership)**. It does not claim
  deadlock-freedom, general liveness, or absence of logical races that are
  not data races.
- Rows 10–16 describe primitives **outside the `.hy` surface**: the
  checker's fail-closed behavior means such values cannot silently cross a
  boundary, but the checker is not an authority on those primitives
  themselves.
- `-race` is cross-checking evidence, never proof — we use it as a backstop
  and a differential oracle, and we do not claim parity with it.
