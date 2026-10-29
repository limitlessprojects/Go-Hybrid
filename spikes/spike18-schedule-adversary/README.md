# W-018 independent schedule adversary

This isolated spike supplies reproducible evidence for milestone 2 of
[issue #378](https://github.com/limitlessprojects/Go-Hybrid/issues/378): an
independent schedule replay checker, exhaustive small-state adversaries, and
guarded sequential Go kernels with explicit FIFO payload budgets.

The original W-018 proposal and prototype are credited to **untalsix, now
0xSixtos**, as recorded in [MOONSHOT.md](../../MOONSHOT.md). This follow-up was
written independently without using the pending spike implementations. It
adds a prefix-equation oracle and a finite marking/mask exploration alongside
the emitted-kernel tests. Synchronous dataflow itself is established work:
[Lee and Messerschmitt, *Synchronous Data Flow*, 1987](https://ptolemy.berkeley.edu/publications/papers/87/synchdataflow/).

## Reproduce

Requires Go **1.26.9** and Python 3 for the implementation-mutant controls.
There are no third-party dependencies or network services.

```sh
cd spikes/spike18-schedule-adversary
GOTOOLCHAIN=local go test -v -count=1 ./...
go vet ./...
test -z "$(gofmt -l .)"
python3 check_mutations.py
```

The Go suite generates four separate Go modules in temporary directories,
compiles and runs their tests, and removes them afterwards. It compares 100
periods with both a separate list-based FIFO interpreter and hand-derived
formulas, checks queued values/order, measures `AllocsPerRun(10000)`, checks
physical array sizes, and deliberately triggers each queue guard. The Python
command copies this spike into temporary directories and requires specific
test failures from three deliberately broken implementations. Those failures
are expected negative controls; the command succeeds only if all are detected.

To inspect one generated kernel and its checked accounting report:

```sh
go run ./cmd/certcheck -in fixtures/multirate.json \
  -out /tmp/w018-multirate.go -package kernel
```

The CLI verifies before writing any generated code and exits nonzero on an
invalid input. The JSON report is on stdout; diagnostics are on stderr. Use
`New()` to initialize a generated kernel and `Step()` once per period; its
return value is cumulative sink totals, ordered by sink actor index. Calls on
one kernel must be sequential.

## Admitted model

Every edge carries `uint64` values, positive integer production/consumption
rates, explicit initial values, and a fixed FIFO capacity. A firing consumes
**all inputs before producing any outputs**, including on a self-loop.

| Primitive | Semantics of one firing |
| --- | --- |
| `source` | No inputs; increment a private uint64 counter (initially 0); output that value. |
| `sum` | Sum every token consumed from all incoming edges; output the sum. |
| `scale` | Sum tokens consumed from its one input edge; multiply by `factor`; output the product. |
| `sink` | No outputs; add every consumed value to a private cumulative total. |

Each outgoing edge receives `produce` copies of the firing's output. All
arithmetic wraps modulo 2^64. This defines batching and fan-out explicitly;
the model does not accept arbitrary Go actor bodies. Unknown operations,
callbacks, blocking/foreign calls, non-integer rates, non-uint64 tokens and
unknown JSON fields have no admission path.

`fixtures/*.json` contain hand-supplied witnesses. The checker does not find
schedules. A certificate includes:

- SHA-256 of the graph's compact Go `encoding/json` representation, binding
  topology, rates, initial values and primitive parameters. This is a content
  digest, not a signature or authentication mechanism.
- An ordered list of actor indices and positive per-actor firing counts.
- Per-edge capacities and exact high-water marks for that witness.
- Total FIFO payload bytes: `8 * sum(capacities)`.

The verifier rejects underflow, overflow, missing firings, nonperiodic final
**token counts**, inaccurate summaries and graph/digest mismatches. Token
values and actor state can change between periods. Capacities may exceed the
observed peaks; neither checker nor emitter claims minimum buffering.

The experiment limits input JSON to 1 MiB, actors to 16, edges to 32, schedule
length to 4,096, rates to 4,096 and each FIFO capacity to 65,536 tokens. These
are conservative engineering limits for this spike, not model theorems.

## Independent adversaries

`oracle_test.go` computes every prefix marking with
`initial + produce * producerFirings - consume * consumerFirings`. It checks
pre-firing availability separately so a self-loop cannot consume its future
output. It never calls the checker's mutable transition implementation.

For **12 explicitly listed graph families**, the test enumerates every
schedule of lengths 1 through 6 and every edge capacity from 0 through 3,
including deadlocked cycles, initialized feedback, unequal rates and an
unbalanced self-loop. A second search exhausts reachable
`(FIFO token counts, fired-actor mask)` states for each positive-capacity,
initially valid assignment, without the six-firing limit. This is not an
enumeration of all graph topologies or of uint64 data states.

`check_mutations.py` requires the tests to catch:

1. A checker that allows self-loops to consume future output.
2. A checker that ignores the declared byte budget.
3. An emitter with its underflow guard disabled.

## Interpretation and trust boundaries

See [PREREGISTRATION.md](PREREGISTRATION.md) and [RESULTS.md](RESULTS.md).
Admission establishes token accounting for **one supplied periodic schedule**.
The generated kernels use fixed arrays, data-independent loops and guarded
FIFO operations. There are no internal channels or goroutines. This supports
bounded FIFO storage and absence of channel-token deadlock under that
schedule, assuming the registry semantics and emitter are correct.

The payload budget excludes indices, actor state, padding, stacks, code and
the Go runtime. Allocation measurements apply to successful `Step` calls,
excluding `New`, verification, emission, reference execution and panic paths.
The checker itself and the independent oracle remain trusted software; finite
tests do not constitute a formal proof.

There is no `.hy` admission/lowering implementation, automatic scheduler,
general Go deadlock guarantee, global buffer optimum, or parallel throughput
claim here. In particular, this does not discharge proposal milestone 1's
requirement for two useful real `.hy` subgraphs. It is a separately reviewable
milestone-2 experiment. The proposal's M1 decision, learning acceptance and
any payment allocation remain with the maintainer.

AI assistance: OpenAI Codex. No independent human validation is claimed.
Repository dual MIT/Apache-2.0 licensing applies.
