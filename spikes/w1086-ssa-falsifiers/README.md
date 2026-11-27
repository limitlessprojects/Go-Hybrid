# SSA names do not make storage affine (#1086)

An independent, pre-registered falsification experiment for
[the ANF/SSA proposal](https://github.com/limitlessprojects/Go-Hybrid/issues/1086).
The hypothesis under test is **ordinary single-assignment dominance alone
can replace ownership checks**, not whether SSA is a useful compiler IR.
This is a negative research result, not a production checker change.
Original proposal credit remains with `ryder777777`; this companion contributes
only the independently constructed falsification experiment.

## Reproduce

From the repository root, with Python 3 and the pinned Go/Rust toolchains:

```sh
python3 -m unittest discover -s spikes/w1086-ssa-falsifiers -p 'test_*.py' -v
python3 spikes/w1086-ssa-falsifiers/dominance.py
GO111MODULE=off go test -race -v ./spikes/w1086-ssa-falsifiers
cd compiler
cargo test --workspace --locked
cargo build -p hyb --locked
cd ..
python3 spikes/w1086-ssa-falsifiers/check_hybrid.py --bin compiler/target/debug/hyb
python3 tools/corpus_harness.py --bin compiler/target/debug/hyb
python3 tools/differential_loop.py --bin compiler/target/debug/hyb
```

Set `--bin` to the actual binary if using `CARGO_TARGET_DIR`.
The Python unit tests run in the existing quick CI job; the Go probe and
real Hybrid witnesses run in the existing compiler job.

## F1: a loop can consume a dominance-valid value twice

The CFG has one static definition of every value:

```text
entry:  map0 = alloc()
        goto header
header: map1 = phi(entry: map0, body: map1)
        branch body, exit
body:   consume(map1)
        goto header
exit:   return
```

`map0` dominates its incoming entry edge. `map1` dominates the body and
the back-edge operand. The phi is legal, including its self operand:
the map is loop invariant. The two-iteration trace executes the same
consume twice on the same allocation. The second execution violates
ownership even though **all dominance constraints still hold**.

The fresh control allocates a new map in the body and consumes it once
per iteration. The same static `alloc` instruction produces two dynamic
allocations, so this control is safe. A rule rejecting every consume in
a cycle would reject this control too. Counting static uses is also
insufficient: F1 has only one static consume. Conversely, consuming the
same value in two mutually exclusive branches is safe; both paths are
tested to guard against treating static use count as linearity.

`fixtures/loop_reconsume.hy` and `loop_fresh.hy` are small corresponding
Hybrid programs. The CFG is hand-written, not frontend-generated; their
relationship is the allocation and task-move sequence, with the bounded
range represented by the chosen CFG path. `check_hybrid.py` pins rejection
with `HYB-E3003` and acceptance respectively on the real current checker.

## F2: aliases remain after every lexical name is assigned once

```text
map0  = alloc()
peer0 = alias(map0)
consume(map0)
write(peer0)
```

Every definition precedes and dominates every use. The write nevertheless
uses the consumed storage. Replace `alias` with a deep `clone`, without
changing any definition/use position, and the program is safe. Dominance
cannot distinguish them: the difference is the operations' storage
semantics. The bounded interpreter models that distinction explicitly,
rather than advertising itself as a dominance-only ownership checker.
The corresponding Hybrid witnesses are `alias_write.hy` and `clone_write.hy`.
The latter copies integer entries into a fresh map explicitly; the model's
`clone` operation abstracts that sequence, not a Hybrid builtin.

This is the same *class* exercised by the existing
[`20-freeze-alias-write.hy`](../../tests/checker-corpus/20-freeze-alias-write.hy),
not a newly discovered soundness bug in main. No competitor implementation
or measurements are used here.

## F3: naming map versions is not a functional map update

`map_put(m0, k, v) -> m1` has two materially different implementations:

- Mutate and return the same Go map: constant-size header handling, but
  reads through `m0` observe the write to `m1`.
- Copy the entries into another Go map before updating: the integer-valued
  probe preserves `m0`, but changes the implementation and its costs.

`map_put_test.go` verifies both behaviors on real Go maps, with `-race`.
The expected alias observation makes the negative probe **pass**; it is
not a failing regression test hidden in the suite. Execution is sequential
and race-free. It measures no throughput, allocations, or heap escapes.
The copying control only establishes preservation for this integer-valued
map; shallow copying maps of reference-bearing values is not a general
deep-copy solution.

## What this changes about the proposed experiment

SSA can remove lexical-name ambiguity and make merges explicit. To delete
ownership state, it still needs a replacement semantic proof: for example,
affine resource/capability tokens propagated through edges and phi nodes,
with consumption and alias/loan constraints, or a storage-isolation scheme.
That proof includes loop reasoning even if it lives in a generic CFG
dataflow solver instead of a surface-AST `check_loop_backedge` function.
Similarly, an in-place map update needs a unique-storage/loan proof, or the
functional update needs a specified copying/persistent representation.

The narrow P1 claim is falsified if F1 or F2 is SSA-valid and has a violating
trace. This does **not** falsify an SSA checker that carries ownership
effects, demonstrate a bug in the existing checker, or prove the whole
proposal infeasible. It identifies the missing proof obligation before
removing existing safeguards.

The next experiment should specify those tokens and an ownership-aware
phi transfer, then compare verdicts and diagnostic spans over the complete
historical corpus before removing any checker hooks. P2 (Go escape parity
for SSA-emitted examples) remains untested: this experiment has no Hybrid
normalizer or SSA Go emitter. The existing corpus/differential runs are
baseline validation only; they are not new-checker equivalence evidence.

See [RESULTS.md](RESULTS.md) for the committed pre-registration and actual
validation results. No ledger acceptance, payout, or maintainer verification
is claimed. Prepared with AI assistance.
