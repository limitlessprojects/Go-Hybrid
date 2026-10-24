> Historical spike record. The 18.7× ratio below belongs only to an already-instantiated experimental guest on the original host. It is not a confirmed speedup of public `Engine.Check`, whose separate 2026-10-18 snapshot measured 2.143 ms p50 versus native 1.204 ms. See [that historical snapshot](validation-2026-10-18/README.md), the [preserved 100-edit samples](validation-2026-10-19/README.md), and the [current M1 decision brief](../../W012_M1_REVIEW.md) for path and source boundaries. Current integrated-head validation is separate.

# Confirmatory Replication Protocol & Results Ledger: W-012 In-Process Analyzer Spike

## 0. Preregistration Notice & Protocol Status
> [!IMPORTANT]
> **Exploratory measurements were already observed before this preregistration.**
>
> In accordance with Go-Hybrid spike measurement protocol, this document registers a formal **confirmatory replication**. The hypotheses, targets, sample counts, and execution commands below were locked in at commit `58ad41ae78f240680525a634aad8309fc9ad7281` prior to executing the 100-iteration confirmatory benchmark and test ledger.

- **Baseline Upstream Commit**: `e2a5f7d5eb90becb210df274765adfa50c99b21b` (`upstream/main`)
- **Preregistration Commit SHA**: `58ad41ae78f240680525a634aad8309fc9ad7281`
- **Target Branch**: `spike/w012-inprocess-analyzer`
- **Protocol Class**: Confirmatory Replication (N=100 iterations per benchmark condition)
- **Primary Representative Metric**: Median (p50) and 90th percentile (p90) latency; mean reported as secondary outlier-sensitive metric.

---

## 1. Preregistered Targets & Validation Criteria

| Target # | Target Scope | Target Hypothesis | Pass Criterion | Confirmatory Result |
|---|---|---|---|---|
| **Target 1** | Corpus Parity | 55/55 fixtures match native CLI as normalized parsed JSON envelopes (input filenames normalized only) | 100.0% parity (0 diffs) | **CONFIRMED (55/55, 100.0%)** |
| **Target 2** | Native CLI Subprocess | Original 15–40 ms assumption overstated; p50 < 8 ms | Documented baseline | **FALSIFIED / OVERSTATED** (p50: 5.038 ms) |
| **Target 3** | Steady-State WASM Speedup | Warm in-process check p50 < 1.0 ms; speedup $\ge 10\times$ | p50 ratio $\ge 10\times$ | **HISTORICAL REUSED-GUEST ONLY** (p50: 0.269 ms, 18.7x); public-path target unproven |
| **Target 4** | Filesystem Cache Break-Even | FS cache reduces compile by $>90\%$; break-even $\le 5$ checks | $N_{\text{break-even}} \le 5$ | **CONFIRMED** (Compile: 12.919 ms, **3 checks**) |
| **Target 5** | Failure-Domain Separation | Failures fail closed to `AnalysisFailure`, disjoint from Reject | 100% pass on 5 fault injections | **CONFIRMED** (5/5 injected faults caught) |
| **Target 6** | On-Disk Analyzer & Position | On-disk discovery works via `os.ReadDir`; exact positions via `Fset.AddFile` | Exact `line:col` in `go vet` | **CONFIRMED** (`sibling.hy:7:13`) |

---

## 2. Confirmatory Test Results (Executed at Preregistered Commit)

### 2.1 Corpus Parity Verification (`node test_wasm_parity.js`)
```text
Loaded WASM: 925098 bytes (903.4 KB)
WASM instantiation time: 2.55 ms
Verifying all 55 corpus files against WASM...
Corpus parity: 55/55 (100.0%)
WASM Check Latency per file:
  mean: 0.823 ms
  p50:  0.528 ms
  p90:  1.298 ms
  p99:  10.456 ms
```
- **Verdict**: **PASSED (100.0% parity)**.

### 2.2 Analyzer Contract & Source Position (`go test -v -run TestAnalyzerEmpiricalPassContext`)
```text
=== RUN   TestAnalyzerEmpiricalPassContext
    analyzer_test.go:62: A. pass.Files count: 1 (Only .go files parsed into Go AST)
    analyzer_test.go:66: B1. pass.OtherFiles: [] (len=0)
    analyzer_test.go:67: B2. pass.IgnoredFiles: [] (len=0)
    analyzer_test.go:73: C. Discovered .hy files via DirScan: [DirScan:bad.hy]
    analyzer_test.go:79: D. PkgPath: github.com/limitlessprojects/go-hybrid/spikes/w012-analyzer/testdata/src/samplepkg, HasTypesInfo: true
    analyzer_test.go:82: E. Reported 3 diagnostic(s) from sibling .hy analysis:
    analyzer_test.go:85:  - [hybrid] Pos=193 (.../bad.hy:6:5): [HYB-E1002] `y` is declared and never used
    analyzer_test.go:85:  - [hybrid] Pos=212 (.../bad.hy:7:5): [HYB-E1002] `z` is declared and never used
    analyzer_test.go:85:  - [hybrid] Pos=220 (.../bad.hy:7:13): [HYB-E3003] `x` is read after ownership was moved away
--- PASS: TestAnalyzerEmpiricalPassContext (0.44s)
PASS
```

### 2.3 On-Disk `go vet -vettool` Execution
```text
$ go vet -vettool=./cmd/hybvet/hybvet ./testpkg
testpkg/sibling.hy:6:5: [HYB-E1002] `y` is declared and never used — Go refuses to build the emitted `y := ...` (declared and not used) (HYB-E1002)
testpkg/sibling.hy:7:5: [HYB-E1002] `z` is declared and never used — Go refuses to build the emitted `z := ...` (declared and not used) (HYB-E1002)
testpkg/sibling.hy:7:13: [HYB-E3003] `x` is read after ownership was moved away
exit status 1
```
- **Verdict**: **Exact on-disk .hy positions: CONFIRMED**.

### 2.4 Failure-Domain Separation & Fault Injection (`go test -v -run TestFailureDomainSeparation`)
```text
=== RUN   TestFailureDomainSeparation
    fault_injection_test.go:87: [PASS] Valid program -> Accept
    fault_injection_test.go:104: [PASS] Rule violation -> Reject (1 diagnostics)
=== RUN   TestFailureDomainSeparation/Fault_WasmTrap
    fault_injection_test.go:122: [PASS] Fault Injection 1 (WASM Trap) -> AnalysisFailure: infrastructure error: WASM trap/panic during execution: wasm trap: unreachable instruction executed
=== RUN   TestFailureDomainSeparation/Fault_MalformedJSON
    fault_injection_test.go:143: [PASS] Fault Injection 2 (Malformed JSON) -> AnalysisFailure: infrastructure error: malformed diagnostic envelope: invalid character 'T' looking for beginning of value
=== RUN   TestFailureDomainSeparation/Fault_InvalidPointerBounds
    fault_injection_test.go:163: [PASS] Fault Injection 3 (Invalid Memory Bounds) -> AnalysisFailure: infrastructure error: out-of-bounds guest memory read: ptr=50000, len=100
=== RUN   TestFailureDomainSeparation/Fault_AllocFailure
    fault_injection_test.go:182: [PASS] Fault Injection 4 (Alloc Failure) -> AnalysisFailure: infrastructure error: guest memory alloc failed: out of memory in guest wasm allocator
=== RUN   TestFailureDomainSeparation/Fault_Timeout
    fault_injection_test.go:198: [PASS] Fault Injection 5 (Execution Timeout) -> AnalysisFailure: infrastructure error: context timeout before instantiation: context deadline exceeded
--- PASS: TestFailureDomainSeparation (0.37s)
PASS
```

### 2.5 Confirmatory 100-Iteration Benchmark Raw Output
```text
================================================================================
W-012 HARDENING BENCHMARK: Wazero vs Native Subprocess (100 iterations/case)
================================================================================
[Baseline] Native CLI Subprocess:   min=4.721ms, p50=5.038ms, p90=5.476ms, p99=5.935ms, mean=5.122ms
[CASE A] Fresh Runtime (No Cache):  total_mean=354.967ms (compile=353.878ms, inst=0.216ms, check=0.772ms)
         Compile Stats: min=339.287ms, p50=346.119ms, p90=371.130ms, p99=417.249ms, mean=353.878ms
[CASE B] Reused CompiledModule:     total_mean=0.723ms (inst=0.078ms, check=0.640ms)
         Total Stats:   min=0.513ms, p50=0.694ms, p90=0.947ms, p99=1.392ms, mean=0.723ms
[CASE C] New Runtime (In-Memory Cache): total_mean=14.437ms (compile=13.384ms, inst=0.237ms, check=0.803ms)
         Compile Stats: min=9.583ms, p50=9.887ms, p90=10.356ms, p99=21.417ms, mean=13.384ms
[CASE D] New Runtime (Filesystem Cache): total_mean=14.356ms (compile=12.974ms, inst=0.172ms, check=0.923ms)
         Compile Stats: min=12.337ms, p50=12.919ms, p90=13.400ms, p99=14.090ms, mean=12.974ms
================================================================================
BREAK-EVEN QUANTITATIVE ANALYSIS (100 ITERATIONS CONFIRMATORY):
 [Primary: p50-based Metrics]
  • Native CLI Subprocess p50:                 5.038 ms
  • Pure Warm WASM In-Process p50:             0.269 ms
  • Representative Steady-State Speedup (p50): 18.7x
  • Latency Advantage per Check (delta_T p50): 4.769 ms
  • Cold Compile Overhead (T_compile_cold p50):346.119 ms
  -> Break-Even (Cold / No Cache, p50):        N_break_even ≈ 72.6 checks (ceil: 73 checks)
  • FS Cached Compile (T_compile_fs p50):      12.919 ms
  -> Break-Even (Filesystem Cache, p50):       N_break_even ≈ 2.7 checks (ceil: 3 checks)

 [Secondary: Outlier-Sensitive Mean Metrics]
  • Native CLI Subprocess Mean:                5.122 ms
  • Pure Warm WASM In-Process Mean:            0.278 ms
  • Ratio of Means (Mean Speedup):             18.4x
  -> Break-Even (Cold / No Cache, Mean):       N_break_even ≈ 73.1 checks (ceil: 74 checks)
  -> Break-Even (Filesystem Cache, Mean):      N_break_even ≈ 2.7 checks (ceil: 3 checks)
================================================================================
```

---

## 3. Protocol Verification Sign-Off
All preregistered targets have completed confirmatory execution:
1. Corpus parity: **100.0% confirmed** (55/55 fixtures matching as normalized parsed JSON envelopes (input filenames normalized only)).
2. Baseline CLI latency: **5.038 ms p50** (Original 15–40ms normal CLI latency estimate was overstated on this host).
3. Representative steady-state speedup: **≈ 17–19x** (18.7x observed at p50: 5.038 ms vs 0.269 ms).
4. Break-even threshold: **73 checks (cold uncached)** vs **3 checks (filesystem-cached)** by p50.
5. Failure-domain isolation: **100% verified across all 5 fault-injection scenarios** (fails closed to `AnalysisFailure`).
6. On-disk Analyzer execution: **Exact source line/col positions confirmed** (`file.hy:line:col` emitted under `go vet -vettool`). Sibling `.hy` discovery relies on host filesystem scanning (`os.ReadDir`). Current spike proves on-disk go-vet/singlechecker integration. It does NOT prove unsaved editor-buffer integration.


## Closeout validation — 2026-10-18

The earlier corpus script checked expected fixture verdicts and the first error rule; it did **not** establish complete native/WASM diagnostic equality. The updated script now invokes the native CLI for every fixture and compares complete parsed JSON envelopes, normalizing only the input filename (including its occurrences in diagnostic notes). This is semantic JSON equality, not raw byte equality.

Executed against the existing native/WASM artifacts from the reviewed baseline:

```text
Corpus fixture agreement: 55/55
Native/WASM normalized envelope parity: 55/55
```

`go test ./...` passes for the analyzer module, including ten malformed-envelope cases, acceptance of additive fields, the original five boundary fault injections, a real minimal guest trap, and a real infinite-loop guest interrupted by a 50 ms context deadline. The allocator-failure test uses a mock error; it does not exhaust a real guest heap. The malformed-output tests now include valid JSON with missing/unknown verdicts and contradictory counts. The Analyzer invokes the same validated boundary as the failure harness and propagates infrastructure errors rather than reporting a code diagnostic. Wazero context cancellation is enabled explicitly.

A rebuilt singlechecker driver reports the expected `sibling.hy:6:5`, `:7:5`, and `:7:13` diagnostics under `go vet -vettool`; exit 1 is the expected rejection of the invalid fixture.

Reproduction (from repository root; build both artifacts from the reviewed baseline first):

```sh
export HYBRID_WASM_PATH="$PWD/compiler/target/wasm32-unknown-unknown/release/hy_wasm.wasm"
export HYBRID_NATIVE_PATH="$PWD/compiler/target/release/hyb"
node test_wasm_parity.js
cd spikes/w012-analyzer
go test ./...
go build -o /tmp/hybvet ./cmd/hybvet
go vet -vettool=/tmp/hybvet ./testpkg
```

Benchmark provenance: all canonical 100-run timing figures above are the recorded snapshot at `b3d73c2c3f211f69e856abd089adc4cad9a21f50`, before this closeout added envelope validation. They were not rerun or relabeled as measurements of the changed wrapper. The added validation cost and end-to-end editor latency remain unmeasured. The parity script's per-file timings are separate and are not the canonical benchmark. Long-duration memory behavior remains unproven. Stock-gopls plugins, unsaved `.hy` buffers, and a completed standalone LSP are not demonstrated.
