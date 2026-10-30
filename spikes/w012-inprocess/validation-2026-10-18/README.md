# W012 validation snapshots — 2026-10-18

> **Latest hosted correctness evidence, not a new benchmark:** [W012 validation run
> 39169356538](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39169356538)
> checked PR head `e029d661d8ce7f79e7f362a7ee97bf3251d6a274` against the
> then-current main: **113/113 full native/WASM envelope parity**, Go race suite,
> and Go vet passed; the [compiler CI](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39169356527)
> passed too. This head also guards declared `.hy` read failures and duplicate
> paths. Earlier hosted run 39143438366 passed 79/79 and is superseded only as
> a corpus-count snapshot. This **60-fixture** benchmark and its original raw
> samples remain unchanged for reproducibility. No new public-path latency
> benchmark or editor/LSP integration result is implied.

## Scope and provenance

- Prior PR #556 head: `aa2d9bdd598d32cb6fd19a30b9fb500f9c96563f`.
- Tested correction: `34c759f91e4ff76c8b6d587208cbf4bea6a51ca0` (code tree `d084a402e25d02ded364d7747fc7f776aa2c50af`). This report is a documentation-only follow-on to that tested commit.
- This branch contains **60** checker-corpus `.hy` fixtures. These results do not claim coverage of a later upstream corpus.
- Cloud Linux x86-64; kernel 6.18.55; Go 1.26.9; Rust 1.99.0; Node 24.19.0; wazero 1.9.0; `GOMAXPROCS=9`. The host reports AMD EPYC 9V74 80-Core Processor, with 9 CPUs visible to this process; the model name does not imply 80 dedicated CPUs. Toolchains were installed from their official distribution sites; Go archive SHA-256 was checked against the official download manifest.
- Native and WASM artifacts were freshly built from the prior PR head using Cargo's locked dependencies and release profile. The correction changes no Rust source, so those artifacts are also the correction's Rust artifacts.
- WASM: 1,138,172 bytes, SHA-256 `5e5c7feb5599e134cb5a21eaf68b4e7bec674e05967b4667a15db3c3d6c18b81`.
- Native binary SHA-256: `256696d7de0fe8fe83e27cf04fab61af68f6c50290f70b17b0b9889d534d149c`.
- [Raw benchmark samples](benchmark-samples.json): 100 samples per case; SHA-256 `2a8d694f9b5cc0b5ffa6ac4617c1f65d15d296a091d82e1532c05c40c8dd85da`.

## Correctness: a real wrapper discrepancy found and fixed

At the prior head, the raw native/WASM envelope comparison passed **60/60**, and the existing Go race suite passed. An additional comparison through the actual Go `Engine.Check` found **58/60** agreement for the fields represented by `CheckOutput`.

The discrepancies were `02-shared-map-managed.hy` and `18-pool-shared-struct-managed.hy`. Both native results had `verdict=accept`, `errors=0`, `warnings=1`, and one `HYB-E8001` managed-safety warning. The Go wrapper kept `warnings=1` but returned zero diagnostics. `CheckSourceWithABI` dropped accepted diagnostics, and `Analyzer.Run` separately reported diagnostics only for rejected source.

The correction retains warnings/notes for accepted source and reports them through the analyzer. Acceptance still means no error diagnostics; infrastructure failures remain distinct. New tests cover the complete branch corpus through the public wrapper, a valid accepted-warning envelope, rejection of an error under the accept invariant, and an actual analyzer pass reporting the managed warning at its exact source position.

After correction:

- Native/WASM complete normalized JSON-envelope equality: **60/60**.
- Go wrapper/native equality for modeled fields: **60/60**.
- `go test -race -count=1 -v ./...`: **pass**, including the existing real trap, active timeout, malformed-envelope, and boundary-failure tests.
- `go vet ./...`: **pass**.
- Real `go vet -vettool` smoke: expected exit **1**, with `sibling.hy:6:5`, `:7:5`, and `:7:13`.
- Benchmark native output now uses the same strict envelope decoder as WASM. Negative tests reject a missing binary, malformed JSON, missing required warning count, missing code/message, invalid span, unknown severity, contradictory rejection, and unexpected exit codes.
- JS parity now frees its guest output buffer after decoding. This removes a harness leak; it does not prove long-duration memory stability.

**Driver caveat:** Go `analysis.Diagnostic` has no separate warning severity. The semantic result remains `accept` with zero errors, but reporting the retained warning makes `go vet -vettool` exit 1. This was checked on a temporary package containing fixture 02: `managed.hy:76:13` reports `HYB-E8001`, exit 1. Thus the host driver exit policy differs from native `hyb check` on warning-only input; it is not full CLI exit-code equivalence.

The Go model deliberately does not represent every native envelope field (for example, diagnostic notes/fixits). Its parity test is therefore explicitly modeled-field parity. The separate JS test compares complete envelopes after filename normalization only. Neither result establishes unsaved-buffer, stock-gopls, or end-to-end LSP integration.

## Current benchmark: actual analyzer method is slower here

Single reject fixture: `08-a3-map-slot-invalidated.hy`; 100 sequential samples per case. Times are milliseconds. Native measurements include JSON capture and validation. Every WASM case enables the same context-cancellation support used by the production wrapper.

| Path | p50 | p99 | Mean |
| --- | ---: | ---: | ---: |
| Native `hyb check --json`, subprocess + validation | 1.204 | 1.796 | 1.256 |
| Public `Engine.Check`, compiled module reused, fresh instance per call | 2.143 | 7.176 | 2.658 |
| Reused-instance boundary check, experimental comparison | 1.097 | 1.521 | 1.151 |

The implemented public wrapper is about **1.78× the native p50 latency** on this host. The already-instantiated comparison is only about **1.10× faster than native** and is not the analyzer's implemented instance-lifetime strategy. It is not evidence of an 18.7× current speedup or sub-millisecond IDE diagnostics.

Cold compilation p50 is **388.557 ms**; filesystem-cached compilation p50 is **11.626 ms**. The filesystem-cache case opens new cache/runtime handles in the same process; it is not a fresh-process startup benchmark. The harness labels its compile-only amortization estimates as excluding instantiation and process startup, and emits no finite break-even when the warm path is slower.

These are **cross-host, changed-harness measurements**, not a direct performance regression comparison with the historical snapshot. The old snapshot used a different machine and predated the currently validated/cancellation-enabled path. No tuning was performed to recover its ratio. One fixture and one batch per case are insufficient to characterize all workloads or timing variance.

## Reproduction

Use the pinned Go/Rust toolchains on PATH. Run from the checkout containing the tested correction; `CARGO_BUILD_JOBS=2` and `GOFLAGS=-p=2` bound build parallelism. Do not set native linker flags on the wasm target.

```sh
export CARGO_BUILD_JOBS=2
export GOFLAGS=-p=2
(cd compiler && cargo build --locked --release -p hyb)
(cd compiler && cargo build --locked --release -p hy-wasm --target wasm32-unknown-unknown)
export HYBRID_WASM_PATH="$PWD/compiler/target/wasm32-unknown-unknown/release/hy_wasm.wasm"
export HYBRID_NATIVE_PATH="$PWD/compiler/target/release/hyb"
node test_wasm_parity.js
cd spikes/w012-analyzer
go test -race -count=1 -v ./...
go vet ./...
go build -o /tmp/hybvet ./cmd/hybvet
go vet -vettool=/tmp/hybvet ./testpkg  # expected exit 1 and three documented diagnostics
go build -o /tmp/w012-bench ./bench
/tmp/w012-bench -iterations 100 -output /tmp/w012-benchmark.json
```

The benchmark also accepts `-root`, `-wasm`, `-native`, and `-fixture` for explicit paths. Run the binary without the race detector for timing. The captured run used `flock /tmp/heavy.lock nice -n 10 timeout 600` around the benchmark. Existing Rust builds emitted an unreachable-pattern warning in `hy-check/src/model.rs`; no Rust warning was changed or suppressed.

## Continuous validation

`.github/workflows/w012-validation.yml` adds a separate, path-filtered job for pull requests and main pushes affecting the compiler, W012 analyzer, corpus, parity script, environment pins, or the workflow itself. It builds native and wasm artifacts, runs full-envelope Node parity, Go race/regression tests, and Go vet. The existing generic CI workflow is unchanged. Actions are commit-pinned; permissions are only `contents: read`; checkout does not persist credentials. There are no secrets, schedules, or performance benchmarks.

Workflow YAML structure, trigger/permission constraints, action references, and shell syntax were checked locally. Its build/test commands were executed locally as documented above. At the time of the original benchmark this workflow had not yet run in GitHub Actions. It has since passed in the hosted run linked above. The hosted run is correctness/race validation, not a repeat of the latency benchmark.

## Implication for M1 review

This strengthens the embedding-boundary evidence by finding and fixing an observable loss of managed-safety warnings. It supports reuse of the Rust checker as an embeddable implementation without a semantic Go rewrite. It does **not** establish an IDE latency win. Keep native CLI as the measured baseline; a next performance experiment should be bounded to a real consumer's request path, with cancellation, diagnostics, concurrency, and state-isolation tests, rather than adopting a pooled/reused instance on timing alone.

Full editor latency, long-duration memory stability, the proposal's 25-file/100-edit workload, and distribution footprint remain unmeasured here. M1 acceptance remains a maintainer decision; these measurements imply no award probability.
