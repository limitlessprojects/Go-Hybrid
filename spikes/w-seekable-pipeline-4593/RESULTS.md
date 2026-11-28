# W-4593 spike results — seekable access to a fixed-rate output series

Every number below is produced by `go test ./... -v` or `go run ./cmd/measure` in
`go-seek/`. Nothing is quoted from the proposal without being re-measured; where our
number differs from the proposal's browser model it is called out.

## 1. Pre-registered hypotheses and falsifiers

| ID | Hypothesis | Falsifier |
|----|-----------|-----------|
| **H1** (reproduction) | The 3-to-2-twice fixture and the length sweep reproduce the proposal's counts: `[10,1,11,14]`, 10,584 in-range comparisons, 1,548 invalid queries that read nothing. | Any mismatch, or an invalid query that touches a word. |
| **H2** (isolation) | A late output coordinate resolves to exactly its dependency set on a 2,250,009-word source, with no prefix read. | One extra index, or a value that disagrees with the dense evaluator. |
| **H3** (refusal) | A stateful stage, an out-of-frame dependency and coordinate overflow are refused with distinct errors. | Any of them accepted, or a refusal that reads the source first. |
| **H4** (cost) | A single late coordinate costs O(dependencies) reads; contiguous windows have high read density; scattered coordinates do not. | Contiguous window density far below 1, or scatter density comparable to the window. |

## 2. H1 — reproduction

`go test -run TestFixtureMatchesTheIssue` and `-run TestSweep`:

| Check | Proposal | This spike |
|---|---|---|
| input 0..8 → outputs | `[10, 1, 11, 14]` | `[10, 1, 11, 14]` |
| input 0..3 → outputs | none, including `At(0)` | none; `At(0)` = `ErrRange`, 0 reads |
| in-range comparisons, lengths 0..128 × 3 patterns | 10,584 | **10,584** |
| invalid-query checks (`k = -1, outLen, outLen+1, 1<<40`) | 1,548 | **1,548**, all with 0 source reads |

The comparison count matches because it is forced by the arithmetic: outputs per source
length n are `2*floor(2*floor(n/3)/3)`, and summing that over n = 0..128 three times is
10,584. The invalid-query count matches only if four out-of-range coordinates are probed
per (length, pattern) pair — 387 × 4 = 1,548. That is an inference about how the
proposal counted, not a claim it documented; if its model probed a different set, the
agreement is a coincidence of totals.

## 3. H2 — isolation on a large source

`go test -run TestLateSeekReadsExactlyTheDependencies`, source of 2,250,009 words →
1,000,004 outputs:

| Query | Value | Source words read | Indices |
|---|---:|---:|---|
| `At(1_000_000)` | 27,000,034 | **4** | `[2250000 2250002 2250003 2250005]` |
| `At(1_000_001)` | 6,750,004 | **1** | `[2250001]` |

Both values equal the dense evaluator's. The index sets equal the hand-derived
composition (`k=1e6`: stage 2 needs stage-1 outputs 1,500,000 and 1,500,002; each needs
two source words), and `SourceIndices(k)` — computed without reading — agrees with what
`At` actually touched on every coordinate tested (`TestSourceIndicesAgreesWithWhatAtTouches`).

A forward evaluator for the same coordinate touches 2,250,009 + 1,500,006 + 1,000,004
words. The accessor touches 4.

## 4. H3 — refusals

| Input | Result |
|---|---|
| stage marked `Stateful` (running sum) | `ErrStateful`, nothing evaluated |
| dependency offset outside the input frame | `ErrDep` |
| stage with `Out` lanes but fewer dependency lists | `ErrDep` |
| arity ≤ 0 | `ErrRate` |
| coordinate space that overflows `int64` (two 1→3 stages, source 2^62) | `ErrOverflow` |
| `k < 0`, `k ≥ outLen`, `k = outLen+1`, `k = 1<<40` | `ErrRange`, **0 reads** |
| short source | fewer outputs by construction — the plan length is derived from the snapshot, so a stale plan cannot be applied behind the caller's back |

The last row is a design property worth stating: `At` recomputes `OutLen` from
`len(snapshot.Words)`, so "the plan says 1,000,004 but the file has 900,000" is not a
state the API can express. The proposal's checked length arithmetic is satisfied by
construction here rather than by a separate check.

## 5. H4 — cost

`go run ./cmd/measure`, 2,250,009 source words, 1,000,004 outputs:

| Measurement | Value |
|---|---|
| single late coordinate, amortised over 200,000 calls | **1.0–1.2 µs/call** |
| single late coordinate, source reads | **4** (0.00018% of the source) |
| 1,000 contiguous outputs `[999000,1000000)` | 2,250 total reads, 2,250 distinct, span 2,250 → **100.00% density** |
| 1,000 contiguous outputs, wall time | 2.3–14.6 ms (varies run to run; ~2–15 µs/output) |
| 1,000 scattered coordinates over the full range | 2,215 total reads, 2,215 distinct over a span of 2,244,990 → **0.10% density** |
| dense evaluator | 125–147 ns/output, 1,000,004 outputs in 125–147 ms |

Two honest readings:

1. **The plan is cheap; this implementation is not free.** The ~1 µs per call is
   dominated by allocating a `Snapshot` read-set and a memo map per query, not by the
   index composition, which is a handful of divisions. A compiler emitting straight-line
   `source[p*(k/q)+d]` reads into the accessor would remove both allocations; treat 1 µs
   as an upper bound on this design, not as the cost of the idea.
2. **Locality is the real variable.** Contiguous windows are perfectly dense — the
   accessor degenerates to a sequential scan of exactly the needed block. Random
   coordinates are 0.10% dense over a 2.24M-word span, i.e. 2,215 scattered reads for
   1,000 outputs. On a file that is 2,215 seeks, which is why the proposal's own cost
   section says to measure scattered reads before adding a file API.

## 6. What this spike does not establish

- **No Hybrid compiler run.** There is no `hy-check` invocation here and no emitted
  Hybrid code; this is an ordinary Go implementation of the same plan. Whether the
  Hybrid front-end can *prove* a stage is pure, stateless and fixed-rate is untested.
- **No file layer.** No `io.ReaderAt`, no byte offsets, no snapshot-consistency
  contract. Coordinate overflow is tested; word- and byte-offset overflow are not,
  because there is no byte API to overflow.
- **No floats, no stateful stages, no data-dependent rates.** Refused by design, and the
  refusal is what the stateful test pins.
- **No concurrency.** A single-threaded accessor; parallel seeking to the same snapshot
  is not measured.
- **Bounded to the fixtures above.** The compiler is N-stage general (a three-stage test
  exercises composition), but only 3→2 stages appear in the large measurements.

## 7. Verdict

**The mechanism reproduces end to end and the refusals hold.** Late coordinates resolve
to exactly their dependency set (4 words out of 2.25M), every invalid coordinate reads
nothing, and the three refusal classes are named and tested. The interesting cost is not
the index arithmetic but locality: a viewer asking for a late sample pays 4 reads, while
a viewer scrubbing a range gets a dense block, and a viewer sampling randomly pays one
seek per output. The open question the proposal leaves — whether Hybrid's checker can
prove the purity and fixed rate that make the plan sound — is untouched by a Go
implementation and remains the thing to test next.
