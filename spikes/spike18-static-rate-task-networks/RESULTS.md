# Spike results — 2026-10-17

## Environment and scope

Go 1.26.9, darwin/arm64, Apple A18 Pro. Toolchain downloaded from go.dev and
verified against its published SHA-256:
`f9bb7c0a02506c5d9bf0d1eb1f7ee6c7684f844ae49558308a0427b830e022cc`.
Python 3.12 / Python 3.9+. This is an independent feasibility spike, not a patched Hybrid compiler.

## Correctness

`python3 -m unittest -v test_planner.py`: **15 tests passed**.

`python3 generate.py`: produced four Go kernels, eight tests, and schedule certificates.

`go test -v ./...`: **9 tests passed**, including four Python-reference
comparisons, four zero-allocation checks, and a channel/static equivalence test.
All four kernels measured **0 allocations/Step** with 10,000 measured runs.

The Python test cases include hand-derived expected outputs, multirate repetition,
joins and initialized feedback, plus rejection of bad rates, unsupported
effects, inconsistent balance equations, deadlocked cycles, forged schedules,
wrong capacities, invalid tokens and an insufficient token-payload budget.

### Certified Buffer Capacities
Buffers certified for the four fixtures:
- Fixture 0 (linear pipeline): **16 payload bytes**
- Fixture 1 (multirate join): **64 payload bytes**
- Fixture 2 (diamond network): **24 payload bytes**
- Fixture 3 (initialized feedback loop): **16 payload bytes**

These counts measure token payload storage (`8 * sum(caps)` bytes for uint64 tokens).
They exclude ring indices, actor state, struct padding, goroutine stacks and the
Go runtime. They are **bounded FIFO token-storage budgets, not total process-memory budgets**.

## Runtime FIFO Guard Verification

In addition to static certificate validation, dynamic assertion guards instrumenting all 9 pop and 9 push ring-buffer methods verify runtime queue invariants across 10,000 periods per kernel (`check_runtime_buffers.py`):
1. **Underflow / Overflow Invariance:** No queue ever underflows (`k.n < 0`) or overflows (`k.n > capacity`).
2. **Periodic Marking Preservation:** Marking before and after each period remains identical across 10,000 periods.
3. **Negative Control Rejection:** Mutating a pop decrement deliberately triggers `panic: buffer overflow` on the subsequent period, confirming guards fail closed.

## Timing — raw results, not a general speedup claim

Command: `go test -run '^$' -bench . -benchmem -benchtime=200ms -count=5`

```text
goos: darwin
goarch: arm64
pkg: example.org/hybrid-static-network-spike
cpu: Apple A18 Pro
BenchmarkStaticPipeline-6      98022068   2.458 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6      89452105   2.268 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.292 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.344 ns/op   0 B/op   0 allocs/op
BenchmarkStaticPipeline-6     100000000   2.316 ns/op   0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1234926 195.2 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1201070 199.6 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 201.1 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 202.7 ns/op     0 B/op   0 allocs/op
BenchmarkChannelPipeline-6     1000000 207.7 ns/op     0 B/op   0 allocs/op
BenchmarkPlainSequential-6    126654616   1.892 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    127008178   1.883 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    128510299   1.865 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    127455594   1.885 ns/op   0 B/op   0 allocs/op
BenchmarkPlainSequential-6    120718197   1.974 ns/op   0 B/op   0 allocs/op
PASS
ok example.org/hybrid-static-network-spike 3.723s
```

Medians: static **2.316 ns/op**, persistent channel round trip **201.1 ns/op** (~86x faster than channels),
plain sequential arithmetic **1.885 ns/op**. All three allocate zero per operation.

The channel worker is persistent: startup/shutdown is excluded, and all paths
perform the same scalar arithmetic. The benchmark is serialized and intentionally
tiny; it measures single-hop communication overhead removal, not parallel throughput.
The comparison with plain sequential arithmetic shows what most of the apparent
benefit is: compiling communication and parallel scheduling away into static sequential
ring-buffer steps for this admitted subset.

## Certificate Boundary Clarification

The generated `certificates.json` represents **replayable schedule witnesses** with witnessed
high-water mark buffer capacities for the concrete schedule produced by the planner.
They are not cryptographic digital signatures, nor do they claim global optimality across
the space of all possible valid schedules. Verification is linear in schedule length and
independent of schedule discovery.

## What remains unverified (Honest Boundary)

- Mapping actual `.hy` AST constructs into this representation; compiler check/emit integration remains future work.
- Useful coverage of Hybrid's real examples, cancellation, blocking I/O and arbitrary foreign calls.
- General heap freedom, total resident memory bounds, stack bounds, real-time latency or fairness bounds.
- The producer's greedy schedule is neither optimal nor a complete bounded-buffer search; the checker certifies only the concrete witness it receives.
