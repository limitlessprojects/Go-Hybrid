# Wildcard #1396 — The packet path does not exist; here is the allocator measurement that does

**Issue:** [#1396](https://github.com/limitlessprojects/Go-Hybrid/issues/1396) · **Status:** design-only; no runtime changes in this PR.

## 0. Verdict up front

The proposal opens with a premise the repository does not support: *"Go-Hybrid's hybrid networking model relies on high-frequency allocations during peak packet ingress."* There is no networking model, no packet path, and no ingress loop. Measured: across every `.rs`, `.go`, and `.hy` file in the tree, the tokens `packet`/`pkt`/`ingress` appear **3 times**, all incidental, in `compiler/crates/hy-check/src/foreign.rs` (a foreign-boundary descriptor table). `sync.Pool` is used **nowhere**. The only benchmark workloads are `benchmarks/workloads/hy/alloc.hy` and `load.hy`.

That does not make the allocator idea worthless — it makes the *claim* unmeasured. The honest version starts from the workloads that exist, and two claims in the proposal fail on their own terms before any code:

1. **"Lock-free … using `sync.Pool`" is a contradiction.** `sync.Pool` is a per-P cache plus a shared victim list; the fast path is cheap but the slow path takes `poolLocal` locks and the runtime drops pool contents at every GC. It is not lock-free, and it cannot guarantee *zero* heap allocations across a GC that clears it.
2. **Fixed size classes (64 B / 512 B / 2 KB / 64 KB) are guesses.** Without a measurement of the actual allocation-size histogram on `alloc.hy`/`load.hy`, there is no reason to believe those four classes cover the hot path, and every miss becomes a heap allocation — the exact thing the design claims to eliminate.

## 1. What exists to measure

`benchmarks/workloads/hy/` contains two programs: `alloc.hy` and `load.hy`. They are the only defensible baseline for any allocator claim in this repo. The measurement plan below uses them unchanged, so the result is comparable to whatever lands later.

The Go runtime already gives us the numbers: `go test -bench . -benchmem` for allocations/op, `runtime.ReadMemStats` + `GODEBUG=gctrace=1` for GC frequency and pause attribution, and `go tool pprof -alloc_space` for the size histogram. No new instrument is required.

## 2. The salvageable design

Replace the transparent `sync.Pool` slab with an **explicit, bounded arena**:

- **Borrow/release, not Get/Put.** A caller receives a `(slice, generation)` pair and must release it; the arena never reclaims silently. This makes "no leak" checkable (generation counter validation, as the proposal wants) and removes the GC-drops-your-buffers problem entirely.
- **Size classes derived from the measured histogram**, not chosen a priori. If 90% of allocations are in two classes, the arena ships with two.
- **Per-P shards with a lock-free fast path on a hit and a bounded fallback on a miss.** A miss must fail closed (return nil / allocate once) rather than spin; a CAS loop under contention is not obviously better than `mcache`, which is already per-P and has no atomics on the fast path.
- **An explicit cap.** An unbounded slab is a memory amplifier under packet floods; the proposal has no cap, which is the single most dangerous omission for a "high-throughput ingress" design.

## 3. Pre-registered tests (before any code)

1. **Baseline first.** Record allocations/op, bytes/op, GC cycles/sec and total pause time for `alloc.hy` and `load.hy` on the pinned toolchain. Every later claim is expressed as a delta against these numbers. If the baseline already has < 1 alloc/op on the hot loop, the premise collapses and the issue should be declined with the number.
2. **Histogram.** Publish the allocation-size histogram for both workloads. Size classes are then justified or the design changes.
3. **The 30% target.** GC overhead reduction ≥ 30% (the proposal's own number) at equal throughput, measured on the same workloads. Below that, decline with the measurement — a failed wildcard still pays M2 if the learning is published.
4. **Contention.** 1 / 4 / 16 concurrent borrowers: throughput must not regress versus the Go allocator; a CAS bitmap that loses at 16 threads is a net loss.
5. **Leak and cap.** Generation-counter validation catches a double-release; a deliberate under-release must exhaust the arena and fail closed, not grow without bound.
6. **Compatibility.** No change to existing buffer interfaces; the arena is opt-in and observable (a metric for hit/miss/fail).

## 4. What does not survive

- **The packet-ingress premise** — there is no packet path in this repository; the motivating workload is hypothetical.
- **"Lock-free `sync.Pool`"** — `sync.Pool` has locks on its slow path and is cleared at GC; it cannot underwrite a zero-allocation guarantee.
- **Fixed, unmeasured size classes** — they convert a design into a guess; misses become the very allocations being eliminated.
- **Unbounded slab** — no cap means an attacker-controlled flood becomes an OOM.

**What survives:** an explicit borrow/release arena, sized from a real histogram, benchmarked on the only two workloads this repo actually has. And if the baseline shows the Go allocator is already good enough, that is a publishable, payable finding — which is exactly what the wildcard track is for.
