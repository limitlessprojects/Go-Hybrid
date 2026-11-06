# W-024 — Certified task-region memory budgets (issue #2965)

Status: **proposal intake, analysis only. No compiler work, no measurement.**
Per BOUNTY.md Track 3 a wildcard needs no prototype; nothing here is a claim of "done".

## Sacred cow
GUARANTEES.md Row 17 (Go GC, unchanged) says nothing about memory exhaustion:
no claim about OOM, task leak budgets, or allocation bounds. Row 1 (race freedom)
is unaffected.

## Proposal
`task(budget = 16.MiB) worker()`, with each spawned task carrying an allocation
certificate derived from the static call graph. A task that cannot be bounded is
either `task(budget = unmetered)` or routed to an isolated arena that fails
gracefully (`ErrTaskMemoryExceeded` / scope cancellation).

## Pre-registered falsifiers (from the issue)
- F1: for a 10k-task fan-out, the static upper bound is within 15% of the
  measured heap delta (`runtime.ReadMemStats`).
- F2: if recursion or foreign Go calls degrade the bound to Infinity on >40% of
  non-trivial code, the static approach is falsified. Fall back to runtime
  arenas with `ErrTaskMemoryExceeded`.

## Open issues to resolve before any build (verify-before-build)
1. **Cost model vs Go GC.** Go heap delta includes GC slack (GOGC), size classes
   and escape-analysis decisions made by `cmd/compile` *after* hy-emit. A bound
   on emitted source allocations is not a bound on RSS. F1 must state which
   quantity is measured (live heap at peak vs total allocated).
2. **Garbage vs live.** A certificate on "bytes allocated" is trivially huge for
   loops. The useful bound is *peak live* bytes, which needs lifetime info that
   hy-check's escape tracker only partly has.
3. **Foreign calls.** Every Go stdlib/third-party call needs an allocation
   summary. Without a summary database (cf. W-016, which avoids one) F2 fires
   almost immediately. Rust-side allocations live outside the Go heap.
4. **Goroutine stacks** are not heap but count toward RSS; 10k tasks x stack
   growth must be in the model.
5. **Enforcement** of the arena fallback: Go has no per-goroutine heap limit, so
   "fails gracefully" needs an explicit allocator API, not a transparent one.

## Suggested spike (smallest falsifier-first step)
Run F2 first, since it is cheap and static: over `examples/` and `tests/corpus`,
count call-graph-reachable functions that contain recursion, an unsummarised
foreign call, or an unbounded loop with an allocation. Report the fraction
against the 40% threshold. Only if it passes, build F1.

## Not done
No hy-check change, no syntax, no tests. Any `.hy` surface (`budget =`) would
need a SPEC-LANGUAGE decision first.
