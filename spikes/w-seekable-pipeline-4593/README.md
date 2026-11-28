# W-4593 spike: a fixed-rate pipeline compiled into a seekable output series

- **Issue:** #4593 — *[wildcard] Compile a fixed-rate pipeline into a seekable output series*
- **Proposal credit:** `MarkDonish`
- **Status:** M2-style learning artifact; verdict and limits in [RESULTS.md](RESULTS.md).

The proposal asks whether a closed, acyclic pipeline of pure fixed-rate stages can
answer "what is output 1,000,001?" without evaluating anything before it. This spike
does exactly that in ordinary Go: each stage declares `(p in, q out)` and a literal
dependency list per output lane, and the plan is composed backwards into a short list
of source indices.

## What is here

| File | What it does |
|---|---|
| `go-seek/pipeline.go` | the compiler: validates stages, composes lane dependencies backwards, exposes `OutLen`, `At(k)`, `SourceIndices(k)`, and a dense `Dense()` reference |
| `go-seek/pipeline_test.go` | the issue's fixture, the 10,584-comparison sweep, late-seek indices, refusals |
| `go-seek/cmd/measure` | every number quoted in RESULTS.md |
| `go-seek/go.mod` | dependency-free module (`go 1.22`) |

## Reproduction

```bash
cd spikes/w-seekable-pipeline-4593/go-seek
go test ./... -v
go run ./cmd/measure
```

Go 1.22+; standard library only; no network.

## Result, short version

- The issue's fixture reproduces exactly: input 0..8 gives `[10, 1, 11, 14]`, the
  length sweep produces **10,584** in-range comparisons and **1,548** out-of-range
  queries, and every out-of-range query reads **zero** source words.
- A late coordinate reads only its dependencies: `At(1_000_000)` on 2,250,009 source
  words reads exactly `[2250000, 2250002, 2250003, 2250005]` (4 words), `At(1_000_001)`
  reads `[2250001]` (1 word). A forward evaluator needs all 2,250,009.
- Refusals are named errors, not silent guesses: a stateful stage (`ErrStateful`), an
  out-of-frame dependency (`ErrDep`), and coordinate overflow (`ErrOverflow`).
- Cost honesty: 1,000 **contiguous** outputs read a 2,250-word block at 100% density;
  1,000 **scattered** coordinates read 2,215 words spread over a 2,244,990-word span
  (0.10% density) — on a file that is 2,215 seeks, which is the cost the proposal warns
  about and the reason a byte-level `ReaderAt` contract is not attempted here.
