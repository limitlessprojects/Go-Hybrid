# What atomic exchange does and does not establish

Independent feasibility learning for [proposal #833](https://github.com/limitlessprojects/Go-Hybrid/issues/833),
original idea by @magomedta. The pre-registered gates and measured results are in
[RESULTS.md](RESULTS.md). This directory is an experiment, not a new language or
runtime surface. No `atomic.Box` sendability or Row 11 guarantee is introduced.

Run with the repository's pinned Go 1.26.9, native C compiler, and Python 3:

```sh
python3 spikes/atomic-box-833/verify.py
```

The compiler CI job runs the same command. Dependencies are standard-library
only. The script first runs the positive tests with `-race` at CPU settings
8, 16 and 32, including one million buffer exchanges per setting. It repeats
the small ownership and progress controls 1,000 times per setting. A separate
intentional negative control must fail with an actual Go race report. A generic
process failure cannot satisfy this control. Finally it measures five
single-CPU, non-race Swap benchmarks; performance is reported, not asserted
across different machines.

## The distinction the experiment tests

For a single cell and disjoint graphs, `Swap(next)` can transfer access:

```text
before: worker owns B; cell owns A
after:  worker owns A; cell owns B
```

That precondition is substantial. The small Go wrapper catches a direct
self-exchange but cannot detect arbitrary interior aliases or revoke copies
of pointers held by callers. Its successful stress tests are runtime evidence
under the caller discipline, not proof that Hybrid enforces that discipline.

Ordinary Treiber pop has a different shape:

```text
h := head.Load()       // observe without acquiring
n := h.Next           // read a field before winning ownership
head.CAS(h, n)        // n is part of the graph already published through h
```

The proposal forbids the first operation on mutable pointees. The replacement
also overlaps the old graph: after successful CAS, `acquired.Next` still reaches
the tail that remains published. `TestTreiberPopOverlapsReplacement` makes that
graph identity explicit using the raw Go API. It intentionally violates the
exclusive-input precondition; it is **not** an accepted Hybrid program or a
counterexample to exchange between disjoint graphs. A checked Treiber API needs
additional structure, such as a protected read-only link capability and an
ownership rule that separates the removed node from its tail.

Taking the entire stack container with `Swap(nil)` avoids that shared read.
However, if the holder stops, every peer gets nil indefinitely. The progress
test executes 10,000 such attempts and checks the unchanged cell state, then
verifies recovery when the holder resumes. Here `tryPop` is one step of the
fallback Pop loop: false means retry, not a completed empty-stack answer;
true with a nil node is the actual empty answer. The repeating nil-to-nil transition
is the reason this fallback is blocking; calling its instructions atomic does
not establish lock-free progress. This is a witness for this fallback, not an
impossibility result for different algorithms with helping or other protocols.

Restoring unchecked Load is not a safe shortcut. The opt-in negative control
loads an old head, pops it in another goroutine, and concurrently reads/clears
its `Next` field. The race detector must report that pointee race. This control
is outside the no-Load subset and must never be presented as its G1 failure.

RCU likewise needs a separate reader-lifetime rule. The Linux RCU documentation
distinguishes removal from reclamation: existing readers can still hold the old
object after replacement, so reuse must await their completion. An exclusive
buffer-exchange test does not validate that reader protocol.

## What remains before a guarantee can change

1. Define the ownership region precisely: whole reachable graph or a node with
   separately protected links. Specify allowed overlap, cycles, and interior
   aliases, and how the checker proves the preconditions.
2. Give `expected` an opaque identity capability if it is usable by competing
   callers. It cannot grant field access. Address node reuse/ABA explicitly.
3. Define conditional ownership in the typing rules: CAS failure retains the
   replacement, success consumes it and acquires the removed object. A boolean
   result alone does not establish branch-sensitive ownership in the checker.
4. Supply a typable lock-free stack and its progress argument, then run the
   million-node, 64-task, `-count=1000 -cpu=8,16,32` campaign from #833. That
   success campaign has not been substituted with the buffer/control tests.
5. Implement and negatively test move, alias, load, and CAS-branch rules in the
   real compiler before changing any sendability class or GUARANTEES row.

## Sources and relationship to existing work

- [Go atomic operations](https://pkg.go.dev/sync/atomic@go1.26.9): the pointer
  operation is synchronized; the experiment tests the separate pointee rules.
- [Linux RCU overview](https://docs.kernel.org/RCU/whatisRCU.html#rcu-overview):
  reader completion separates removal from safe reclamation.
- [Existing companion PR #1254](https://github.com/limitlessprojects/Go-Hybrid/pull/1254)
  is closed and unmerged. Its implementation and evidence were not reused.
- [W-022](https://github.com/limitlessprojects/Go-Hybrid/issues/544) is the broader
  synchronization-capability family already named in MOONSHOT.md. This note
  credits that family and tests the particular Treiber expressibility/progress
  claim in #833; it does not claim a new wildcard allocation or payout.
