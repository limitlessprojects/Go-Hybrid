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

_(filled after the change is implemented and measured — nothing below
this line was seen before registration)_
