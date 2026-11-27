# Atomic exchange feasibility for #833

Original proposal: https://github.com/limitlessprojects/Go-Hybrid/issues/833
Original proposal credit: @magomedta. This is an independent feasibility experiment.
Base: `cc59624818a7e2298e3b7d977762600192cbaf77`.

## Pre-registration

Before executing this experiment, record these gates:

1. **Exchange safety control:** 16 producers and 16 consumers exchange distinct
   mutable buffers using only Swap. Each caller stops accessing its outgoing
   graph and may mutate only the returned graph. Check at least 1,000,000
   exchanges with the Go race detector. A race or conservation failure kills
   this implementation. This is evidence, not a static ownership proof.
2. **Conditional CAS:** a failed CAS returns no acquired pointee and leaves the
   replacement with its caller; a successful CAS returns the displaced pointer.
   Cover nil, success, failure and competing CAS without dereferencing an
   unowned expected pointer. Expected is an identity, not an ownership proof.
3. **Treiber expressibility falsifier:** ordinary pop must read head.Next before
   CAS; the proposal forbids non-consuming mutable Load. Establish the missing
   operation with a concrete two-node reachability witness. Do not call this a
   Hybrid checker regression: atomic.Box has no language implementation yet.
4. **Take-and-return fallback progress falsifier:** if a task is paused after
   taking the sole stack container, schedule another task for 10,000 attempts.
   If the cell stays nil and no operation can complete, the retry transition
   has a non-progress cycle. This fallback is a spinlock, not a lock-free stack.
   The witness concerns this fallback, not an impossibility theorem for all
   lock-free algorithms or all future atomic ownership designs.
5. **Raw-CAS negative control:** a separate opt-in test permits the conventional
   Treiber Load, then clears Next after a successful pop. Expect go test -race
   to report a pointee race with an observer that already loaded the old head.
   Require a real race report, not just a nonzero process exit. This shows why
   adding unchecked Load to recover expressibility is not a safe remedy; it
   is deliberately outside the proposed no-Load subset.
6. **G2 timing:** benchmark uncontended Swap without -race, CPU=1, five samples;
   report ns/op and allocations, actual arm64 platform and Go version. Apply
   the proposal thresholds (<15ns target, >30ns kill, intermediate inconclusive)
   only to these measurements. Do not import x86 LOCK CMPXCHG or channel timings.

Decision: report the measured gates honestly. If the Treiber fallback loses
lock-free progress, publish that negative learning rather than flip Row 11 or
add a sendability classification the compiler has not proved. The full proposed
1000 x (8,16,32 CPUs) million-node Treiber success campaign belongs after a
typable algorithm exists; it cannot validate a forbidden Load or a spinlock.

## Results

Pending execution of the pre-registered experiments.
