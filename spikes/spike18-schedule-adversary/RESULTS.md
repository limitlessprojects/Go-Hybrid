# W-018 schedule-adversary results

Date: 2026-10-29. Environment: Linux/arm64, Go 1.26.9, Python 3.12.3;
upstream regression checks used Rust/cargo 1.99.0. No toolchain was downloaded
for this experiment. [Preregistration](PREREGISTRATION.md) was committed as
`a4b4b5f` before execution; upstream base was
`1f0ff978f4deff78c77bca96db0e3e46f32de71b`.

## Token-accounting adversaries

Actual output from `go test -v -count=1 ./...`:

```text
exhaustive witnesses: 25152 checked, 139 accepted, 25013 rejected (12 graph families, capacities 0..3, lengths 1..6)
reachable states: 299; attempted transitions: 738; feasible capacity assignments: 34; infeasible: 25
```

There were no disagreements between replay and the independent prefix-count
oracle. The separate finite search visits all reachable marking/mask states
for the 59 initially admissible positive-capacity assignments, beyond the
six-firing witness bound. The counts describe the explicitly enumerated
families; they are not coverage claims for every small topology or data state.

Mutation/admission tests also rejected altered rates, initial markings,
semantics/digests, repetitions, schedules, capacities, peaks, byte budgets,
unmodeled actors and malformed input. The implementation-mutant command
reported:

```text
KILLED self_loop_consumes_future_output (expected test failure)
KILLED unchecked_payload_budget (expected test failure)
KILLED missing_runtime_underflow_guard (expected test failure)
PASS: all 3 deliberately broken implementations detected
```

These specific failures confirm that the independent oracle and runtime
negative controls can detect those defects. They do not prove that all
checker/emitter defects would be caught.

## Emitted kernels

All four generated Go modules compiled and passed. For each fixture, 100
periods matched both the independent list-based FIFO interpreter and the
formula below; queued values/order matched after each period. All 18
per-edge underflow/overflow negative controls panicked with the expected
diagnostic. `AllocsPerRun(10000)` measured **0 allocations per successful
Step** in each fixture, after initialization.

| Fixture | Cumulative sink value after n periods (modulo 2^64) | FIFO arrays | Complete Kernel (`unsafe.Sizeof`) |
| --- | --- | ---: | ---: |
| Pipeline | `3*n*(n+1)/2` | 16 bytes | 80 bytes |
| Multirate | `9*n*n+3*n` | 48 bytes | 112 bytes |
| Join | `n*(n+1)` | 24 bytes | 120 bytes |
| Initialized feedback | `2^(n+1)-2` | 16 bytes | 72 bytes |

The generated tests summed the physical array sizes and checked equality
with the declared payload. The complete struct includes ring indices/counts
and source/sink state. Neither column includes allocation headers, stacks,
code or the runtime. The feedback case exercises uint64 multiplication and
sink-total wraparound; periodicity concerns queue counts, not token values.

An actual CLI smoke check emitted the multirate kernel, compiled it in a
separate module, ran 10 periods, and obtained 930. An invalid one-byte budget
returned nonzero and produced no output file. `go vet ./...` and the gofmt
check passed.

## Upstream regression checks

On the same local checkout:

- `cargo test --workspace`: 1,800 passed, 0 failed, 0 ignored.
- `cargo build -p hyb`: passed.
- Checker corpus: **127/127**, managed tripwire not fired.
- Full differential loop: **41/41** accept-side build/run, **86/86** rejects.

Two initial differential attempts had Go VCS-stamping failures in temporary
bridge workspaces. The sandbox has an empty `/tmp/.git` directory; the Go
tool detected it but Git could not obtain repository status. An attempted
`GOFLAGS=-buildvcs=false` correction had no effect because hyb sets GOFLAGS
itself. The successful full run used `TMPDIR` under the real cloned repository.
No compiler, corpus, harness, verdict, or expected output was changed. The
initial failures and successful rerun remain in the local test log.

To reproduce that environment arrangement from the repository root, with
the pinned Go and cargo binaries on PATH and `HYB_BIN` pointing to the built
compiler:

```sh
mkdir -p .validation-tmp
TMPDIR="$PWD/.validation-tmp" python3 tools/differential_loop.py --bin "$HYB_BIN"
rmdir .validation-tmp
```

The dedicated ARM64 workflow repeats the spike suite, vet/format checks and
implementation-mutant controls. Hosted CI is **pending**; no PR has been
published by this local preparation.

## Decision and remaining falsifiers

The bounded experiment supports a supplied schedule's FIFO accounting and
the four emitted kernels' agreement with the registered semantics. It does
not establish a complete scheduler, globally minimal buffers, correctness of
arbitrary actors, source-to-kernel lowering, general Go liveness, or parallel
performance. The primitive registry, replay implementation and emitter are
still trust boundaries. No speedup was measured or claimed.

This is milestone-2 supporting evidence for the original W-018 proposal.
Milestone 1's two useful real `.hy` subgraphs remain untested here; there is
no basis in this experiment to make the kernel backend the default. A
counterexample within the admitted semantics still invalidates its safety
claim. Maintainer review, wildcard selection, learning acceptance and any
reward allocation remain pending.

AI assistance: OpenAI Codex. No independent human review is claimed.
