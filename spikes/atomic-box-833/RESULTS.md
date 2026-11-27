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

Measured 2026-11-27 with **Go 1.26.9, linux/arm64**, native GCC, no
external services. The pre-registration was committed as `a1aef77` before running
these experiments. The original local Go installation lacked `runtime/race`;
that first attempt failed at build time and is not counted as a test result.
A clean official `go1.26.9.linux-arm64.tar.gz` was downloaded, SHA256 verified
(`4a97373d49fcacdcf3694fea368a500b00ee3e963974f3e7514132717632f052`),
and all experiment stages below were then executed successfully.

| Gate | Observed result | Interpretation |
|---|---|---|
| Exclusive buffer exchange | 1,000,000 exchanges at each of CPU=8,16,32; 33 unique buffers conserved, payload integrity checked, 0 race reports | Positive runtime evidence for disjoint whole-graph exchange under caller discipline |
| CAS outcomes and contenders | Failure retains replacement and returns nil; success acquires old pointer; exactly one winner among 64 contenders | Runtime contract controls pass; no static ownership or ABA proof |
| Small controls stress | Five top-level controls, each 1,000 repetitions at CPU=8,16,32; `-race` pass (11.925s) | 3,000 executions of each small control, excluding the separate million-exchange tests |
| Treiber graph witness | CAS(head, tail) returns head whose Next equals the still-published tail | This replacement violates the exclusive-input premise; ordinary Treiber pop needs a richer ownership rule |
| Take-and-return progress | 10,000 peer attempts complete zero operations, return to the same nil state; operation succeeds after owner resumes | The fallback has a repeatable non-progress cycle and is blocking |
| Unchecked Load negative | Actual Go `WARNING: DATA RACE` on Next; expected test process failure, checked separately | Restoring raw Load is unsafe; this is deliberately outside the proposed no-Load subset, **not** its G1 falsification |
| G2, non-race single-CPU Swap | 10.85, 10.83, 10.83, 11.20, 11.58 ns/op; every sample 0 B/op, 0 allocs/op | Meets <15 ns target on this host; >30 ns kill threshold did not fire |

Exact machine-readable measurements: [measurements.json](measurements.json).
Actual negative-control output: [race-report.txt](race-report.txt). The full
unmodified local run transcript is retained by the submitting worker; CI runs
[verify.py](verify.py) to reproduce the experiments.

## Learning outcome

**The exchange-only buffer experiment succeeds; the direct route from that
result to a guaranteed lock-free Treiber stack fails its expressibility and
progress prerequisites.** The safety invariant for the positive control is
simple: the cell and each worker own disjoint graphs before every exchange;
atomic Swap permutes those owners without duplicating an owner. This reasoning
depends on the caller satisfying that invariant. The Go wrapper itself cannot
make an arbitrary raw pointer unique, revoke aliases, or validate a graph.

For the proposed Treiber case, banning Load removes the observation of Next
needed to form the CAS replacement. Taking the full graph instead recovers
exclusive field access but makes all peers depend on the holder resuming. The
nil-to-nil retry transition repeats without changing the state, so adding more
iterations cannot establish lock-free progress for that algorithm. This is
stronger than merely observing a slow test, and does not depend on a timeout.
It also does not rule out a future design with helping, protected immutable
links, graph separation, or an independently specified reclamation protocol.

A CAS expected pointer must be treated as an opaque identity until acquisition;
CAS success alone does not grant exclusive ownership of a tail that remains
reachable from both the returned node and the published head. These requirements
are not addressed by naming a type Sendable Class 2. The detailed example and
remaining proof obligations are in [README.md](README.md).

**Disposition:** retain GUARANTEES Row 11 and raw pointer fail-closed behavior.
Publish this as a feasibility learning for #833, with maintainer review and any
award undecided. No ledger ID, M1 acceptance, M2 payment, compiler diagnostic,
or human validation is claimed. This is an independently executed research
artifact, not a reuse of #1254's model or evidence.

## Limits against the original proposal

- The raw-Load negative does not kill the disjoint exchange hypothesis: it
  intentionally adds an operation the proposed checked subset excludes.
- The issue's 64-task, million-node Treiber implementation and its full
  `-count=1000 -cpu=8,16,32` success campaign were **not run**. A typable lock-free
  algorithm is missing under the stated API; the fallback tested here is not
  presented as that algorithm. Three million buffer operations and the repeated
  small controls must not be represented as satisfying that Treiber campaign.
- No compiler classification, method shape, move/alias/loan enforcement,
  conditional CAS typing, emitted Go, or four requested M2 corpus verdicts are
  implemented or claimed. No production `runtime/go` API is added.
- Timing is host-specific and excludes `-race`. No channel baseline, x86
  instruction, RCU grace period, or end-to-end lock-free throughput was measured.
- G1 is evidence from executions, not a proof. The new progress witness concerns
  the explicit take-and-return algorithm, not every possible data structure.
