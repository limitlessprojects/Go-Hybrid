# Histogram recording: replace a manual scan with bit length

## Problem and change

`Histogram.Record` runs in the benchmark instrumentation path. The old
`leadingZeros` helper scans down from bit 63 on every positive sample, so small
nanosecond samples traverse most of that loop. Go's standard `math/bits.Len64`
returns the same magnitude directly: for positive `x`, the exponent is
`bits.Len64(x) - 1 == 63 - leadingZeros(x)`.

Use that expression and remove the local scan. The existing early return for a
zero sample and rejection of negative samples are unchanged. Bucket layout,
counts, mean and quantile code are unchanged. No new dependency or allocation.
This does not fix the separate aggregate-overflow or quantile reports.

## Source and method

Baseline: `f993988a57a2f17e988ea27d31fba910fb16c9dd` plus the new benchmark only.
The candidate changes only exponent selection in the runtime utility, adds the
benchmark and these records. No compiler grammar or verdict change.

Observed on 3 November 2026: Apple M2, macOS arm64, Go 1.26.9. Each case cycles
through three sample values (3/5/7 ns, 900/1000/1100 ns, or
900000/1000000/1100000 ns). Histogram allocation/initialization is outside the
timer. The benchmark checks that exactly `b.N` samples were recorded. Both
versions use the same benchmark bytes, toolchain, cache and machine. Go chooses
`b.N` independently for each calibration; results below are three observations
per case, summarized with the median.

From the repository root, with Go 1.26.9 on PATH:

```sh
GO111MODULE=off go test -count=1 -v ./runtime/go/hybench
GO111MODULE=off go test -run '^$' -bench '^BenchmarkHistogramRecord$' -benchmem -benchtime=250ms -count=3 -cpu=1 ./runtime/go/hybench
```

The actual runs used an explicit task-owned GOCACHE/GOPATH/TMPDIR,
`GOTOOLCHAIN=local`, `GOPROXY=off` and `GOMAXPROCS=2`; `-cpu=1` selects one
benchmark processor. The measurement outputs are [baseline.txt](baseline.txt) and
[candidate.txt](candidate.txt); only the local workspace path is redacted.
Original unredacted command outputs are privately retained.

| Sample group | Baseline median ns/op | Candidate median ns/op | Observed ratio |
| --- | ---: | ---: | ---: |
| nanosecond | 26.140 | 2.866 | 9.12× |
| microsecond | 24.060 | 2.899 | 8.30× |
| millisecond | 19.760 | 2.852 | 6.93× |

All observations reported zero B/op and zero allocs/op. These are local
histogram-ingestion measurements, not end-to-end benchmark speedups, an
architecture-independent promise or a Linux result. CPU scheduling, compiler
version and benchmark context can affect the values. No statistical significance
or maximum-throughput claim is made.

## Correctness and validation

The five existing histogram tests passed on both versions: ranks/counts,
resolution across magnitudes, window means, empty/negative/zero handling and
summary shape. A separate equivalence probe compared the old and new exponent
expressions at 187 distinct nonnegative `int64` values: zero, MaxInt64, and every
power of two with its immediate neighbours. Every comparison matched. This
covers bit-length boundaries; it is not an exhaustive enumeration of all inputs.
The mathematical equivalence for positive values follows from the definitions
of leading-zero count and bit length; the zero fast path is retained.

On tested code commit `0860ff59b851c920fcc812317df5fec7b6f925f1`, the required
local checks completed successfully with the pinned Rust 1.99.0 and Go 1.26.9:

- `cargo test --workspace --offline --quiet`: 2,584 passed, zero failed/ignored.
- `cargo build -p hyb --offline --quiet`: passed.
- `tools/corpus_harness.py --bin <hyb>`: verdict agreement 130/130.
- `tools/differential_loop.py --bin <hyb>`: 42/42 build-and-run and 88/88 rejections.
- `bash tools/run_examples.sh`: all ten examples passed (including both cat paths).
- `git diff --check`: passed; the changed Go files were formatted with gofmt.

Commands were invoked directly with correctly quoted argv and task-owned paths,
using the existing offline dependency caches. Subsequent documentation updates
preserve the two tested Go files byte-for-byte. Hosted CI, maintainer review,
merge and any discretionary bounty award remain unconfirmed.

Provenance: AI-led source review, patch, benchmark and command execution. No
independent human review is claimed. Existing separate findings/PRs are credited
to their authors and are not reproduced or claimed here.
