# Spike 7: Zero-copy string arguments across the bridge

**Status:** hypothesis and thresholds pre-registered 2026-11-03 BEFORE the
change was implemented or measured (spike protocol, MOONSHOT.md "Honest
limits"). The baseline section below was measured on unmodified main
@ ddd2172 and is part of the registration, not of the result.
**Question:** can a string argument (a JSON key, a regex pattern/text)
cross into hyrt as a raw {ptr, len} view of its own backing bytes — no
`[]byte(s)` copy, no `runtime.Pinner` — and stay sound under GC and stack
pressure?
**Origin:** wildcard proposal #3429 (AhmedB03) proposes static pin
certificates for borrowed slices. The baseline audit (below) found the
large-buffer half of that proposal already implemented; this spike tests
the one crossing that still copies.

## Baseline audit (measured / read on main @ ddd2172, before any change)

- Large buffers already cross zero-copy: `sha3binding.Digest256` and
  `jsonbinding.Parse` pin the caller's own array and pass {ptr, len}
  (the pinned-borrow protocol, interfaces/abi-types.md §2). There is no
  O(N) copy on the sha3 or JSON-payload path to remove, and the per-call
  pin cost (~55 ns ×2) is under 1% of a 64 KiB hash — #3429's "≥3×
  throughput on ≥64 KiB payloads" metric has no copy to eat and is not
  reachable against this baseline.
- The one real per-call copy: string arguments.
  `jsonbinding.pinCrossKey` and `regexbinding.pinString` both do
  `b := []byte(s)`, then pin the copy. Every keyed JSON call
  (Int/String/Array/PushString/Set) and every regex call pays it.
- Escape analysis of the keyed-read path (`go build -gcflags=-m`)
  attributes BenchmarkGetIntByKey's 3 allocs/op to exactly:
  1. the `[]byte(s)` conversion in pinCrossKey,
  2. pinCrossKey's `runtime.Pinner` escaping to the heap (returned by
     pointer),
  3. the pinned out-parameter (`&n`) moving to the heap — the
     pinned-borrow protocol for RESULTS, not the string argument; out
     of scope for this spike.

## Hypothesis

A Go string's backing bytes can cross as {ptr, len} via
`unsafe.StringData(s)` with no copy and no pin, provided:

- the string is immutable (language guarantee) and the callee contract
  is the §9 O3 view loan: read-only, never retained past the call;
- the caller holds the string with `runtime.KeepAlive(s)` until the
  crossing returns, so the backing array is reachable for the whole call;
- heap-backed strings cannot move mid-call (Go's collector is
  non-moving), and literal strings live in rodata.

The open risk — the spike's real question — is a STACK-backed string:
if a string's bytes sit in a goroutine stack frame and that stack is
copied (growth/shrink) while Rust holds the raw pointer, the pointer
dangles. The stress test below is built to provoke exactly that
(GC + deep recursion around keyed calls). If the stress test fails, the
honest conclusion is "zero-copy is sound only for heap/rodata-backed
strings; short stack-resident strings keep the copy" and the change
must not land as-is.

## Pre-registered thresholds

| # | Claim | Threshold |
|---|-------|-----------|
| T1 | The string crossing itself allocates nothing | BenchmarkGetIntByKey allocs/op: 3 → ≤1, and the remainder is attrib. (3) only (the pinned out-param). BenchmarkIsMatch allocs/op: 5 → ≤1. B/op strictly lower on both. |
| T2 | No throughput regression | BenchmarkGetIntByKey and BenchmarkGetIntByLongKey median ns/op within +10% of baseline (expected: small improvement; keys are short, so ns is the weak signal, allocs the strong one). |
| T3 | Correctness under pressure | jsonbinding + regexbinding suites pass; the new stress test (keyed calls interleaved with `runtime.GC()` and stack-growth recursion, result-checked every iteration) passes under a normal run, under `-race`, and under `GODEBUG=gcstoptheworld=1`. |
| T4 | Empty-string semantics unchanged | (0, 0) still crosses for "" (TestEmptyKeyEdge and friends pass). |

Decision rule: T1+T3+T4 all green → land the change in both bindings.
Any T3 failure → do not land; this file records the falsification and
the fallback (copy kept, or zero-copy gated to provably heap-backed
strings) as the spike's knowledge output.

## Method

- Instrument (committed with this registration):
  `runtime/go/jsonbinding/bench_test.go` (BenchmarkGetIntByKey,
  BenchmarkGetIntByLongKey) and `runtime/go/regexbinding/bench_test.go`
  (BenchmarkIsMatch).
- `go test -run '^$' -bench . -benchmem -count=5`, median of 5.
- Toolchain: Go 1.26.9 (pinned), libhyrt release build from this tree.
- Machine note: measured on a shared 4-vCPU sandbox (GOMAXPROCS=4),
  NOT the maintainer's pinned 2-core box — absolute ns/op are
  sandbox-measured; the claim is the before/after delta on one machine.

## Baseline results (unmodified main @ ddd2172, median of count=5)

| benchmark | ns/op | B/op | allocs/op |
|-----------|------:|-----:|----------:|
| BenchmarkGetIntByKey-4 | 383.0 | 24 | 3 |
| BenchmarkGetIntByLongKey-4 | 318.6 | 80 | 3 |
| BenchmarkIsMatch-4 | 47502 | 96 | 5 |

## After results

Change: `pinCrossKey` / `pinString` replaced by `crossString`
(`unsafe.StringData` + {ptr, len} + caller `runtime.KeepAlive(s)`; no
copy, no pin for the string argument; out-parameters still pinned by
callers per the pinned-borrow protocol). Same method, same machine,
median of count=5:

| benchmark | before ns/op | after ns/op | before B/op | after B/op | before allocs/op | after allocs/op |
|-----------|-------------:|------------:|------------:|-----------:|-----------------:|----------------:|
| BenchmarkGetIntByKey-4 | 383.0 | 158.3 | 24 | 8 | 3 | 1 |
| BenchmarkGetIntByLongKey-4 | 318.6 | 160.2 | 80 | 8 | 3 | 1 |
| BenchmarkIsMatch-4 | 47502 | 45414 | 96 | 0 | 5 | 0 |

Threshold verdicts:

- **T1 PASS.** GetIntByKey 3 → 1 allocs/op; post-change escape analysis
  (`go build -gcflags=-m`) attributes the remainder solely to the
  pinned out-parameter (`moved to heap: n`) — the string crossing
  itself allocates nothing: no `[]byte(s)` escape, no Pinner escape.
  IsMatch 5 → 0 allocs/op. B/op strictly lower on all three.
- **T2 PASS, with margin.** No regression anywhere: −59% ns/op on the
  short key, −50% on the 64-byte key, −4.4% on IsMatch (whose time is
  dominated by Rust-side pattern compilation). The ns win on keyed
  reads is larger than registered expectations — the copy plus Pinner
  bookkeeping was over half the cost of a ~380 ns crossing.
- **T3 PASS.** jsonbinding + regexbinding suites pass normally and
  under `-race`; TestCrossStringUnderPressure (2,000 iterations of
  result-checked keyed calls interleaved with `runtime.GC()` and
  1,200-deep stack-growth recursion; 300 iterations for regex) passes
  normally, under `-race`, and under `GODEBUG=gcstoptheworld=1`.
- **T4 PASS.** TestEmptyKeyEdge and the full suites pass; the (0, 0)
  empty-string crossing is unchanged.

Honest limits:

- The stack-backed-string risk could not be exercised directly:
  deliberately stack-resident argument strings cannot be constructed
  from Go source (string conversions escape to the heap), so the stress
  test covers literal (rodata), heap, and substring backings under GC +
  stack-copy pressure on the calling goroutine, not a proven
  stack-resident backing. Note a crossing runs inside the NOSPLIT
  trampoline, and a goroutine's stack is only copied while that
  goroutine executes Go code — but that argument is analysis, not
  measurement, and is recorded as such.
- Absolute numbers are from a shared 4-vCPU sandbox, not the pinned
  2-core box; the deltas, not the absolutes, are the claim.
- Out-parameter pinning (the remaining 1 alloc/op on keyed reads) is
  untouched — it is the results half of the pinned-borrow protocol,
  not the string-argument copy this spike targeted.
