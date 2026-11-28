# W-4843 Spike Results — Monotonic Ownership-to-Value Sealing (`seal move` -> `sealed[T]`)

**Proposal:** [#4843](https://github.com/limitlessprojects/Go-Hybrid/issues/4843) | **Track:** Wildcard
**Verifiers:** `python3 -m unittest discover -s spikes/w4843-seal-move-sealed-value/tests -v` &
`cd spikes/w4843-seal-move-sealed-value/work && go test -v -count=1 ./...`

## Executive Summary

`seal move x` consumes exclusive ownership of a dynamically-built `map`/`slice`/`struct`
and returns a `Copy`, read-only, zero-allocation projection `sealed[T]` that both the
sender and arbitrarily many `task` receivers may keep reading — closing the gap between
plain `move` (bans sender reads, corpus #19) and `managed` (unchecked shared mutability).

This spike splits the proposal into its two independently-verifiable halves and verifies
each with an executable companion:

- **`checker/seal_checker.py`** models the three *static* admission rules a real
  Tier-0 checker would run before accepting `seal move x`: exclusive ownership
  (`HYB-E3004`), interior representation purity (`HYB-E3015`), and storage-component
  alias exclusivity (`HYB-E3003`) — plus the corpus #19 twin and the GUARANTEES.md
  Rows 5/9 foreign-callback flip (`HYB-E3013`).
- **`work/`** is a real Go package (`SealedMap[K,V]`, `SealedSlice[T]`) proving the
  *runtime* representation: O(1)/zero-allocation sealing, multi-worker broadcast with
  the sender still reading, and the two runtime-shape falsifiers the proposal declares
  (`F1`, `F3`). It also demonstrates, rather than just asserts, why `F2` is **not**
  fixable at the runtime-shape layer alone — which is exactly the argument for keeping
  the storage-component check in the static checker.

## 1. Pre-Registered Hypotheses & Verification Matrix (P1–P3)

| ID | Statement | Verification | Status |
|---|---|---|---|
| **P1** | `seal move` is an O(1), zero-allocation ownership-to-value transition whose witness broadcasts across concurrent workers while the sender keeps reading (the missing twin of corpus #19). | `TestP1_SealTransitionBroadcastsAcrossWorkersSenderStillReads` (Go); `TestCorpus19Twin.*` (Python) | **PASS** |
| **P1 (surface)** | `sealed[T]` exposes no mutation method, so "reject post-seal mutation" is an encapsulation property, not a runtime check. | `TestP1_SealedTypesExposeNoMutationMethods` (Go) | **PASS** |
| **P2** | Sealing is asymptotically cheaper than W-005's clone-at-boundary baseline. | `BenchmarkSealMap10kNodes` / `BenchmarkCloneMap10kNodes`, `BenchmarkSealSlice10kNodes` / `BenchmarkCloneSlice10kNodes`, `BenchmarkBroadcast8WorkersSealMove` / `BenchmarkBroadcast8WorkersCloneBaseline` (Go, see §3) | **PASS** |
| **P3** | A sealed capture handed to a foreign callback boundary has zero write paths; a non-const, non-sealed capture is rejected (`HYB-E3013`). | `TestP3_SealedForeignCallbackCaptureHasNoWritePath` (Go); `TestRow9ForeignCallbackFlip.*` (Python) | **PASS** |

## 2. Declared Falsifiers & Remedies (F1–F3)

| ID | Falsifier | Remedy | Verification | Status |
|---|---|---|---|---|
| **F1** | A slice sealed without clipping spare capacity lets two independent "read-only-looking" `append`s silently clobber the same backing-array slot. | **R1:** `seal move` on a slice clips to `s[:len(s):len(s)]` (`cap == len`), so any later growth must allocate. | `TestF1_NaiveUnclippedSealSliceAppendSpareCapacityRace` (fires) / `TestF1Remedy_ClippedSealSliceAppendNeverAliases` (holds) | **FIRED AS PREDICTED, REMEDY HOLDS** |
| **F2** | A pre-seal sub-slice alias (`leaked := buf[:1]`) that outlives the outer `seal move` witness can still mutate "sealed" data through the shared backing array. | **R2:** `seal move x` must be rejected *statically* whenever `aliases(x) ∪ may_share(x)` contains another live binding — the runtime shape cannot recover from an alias that predates sealing. | `TestF2_PreSealSubsliceAliasSurvivesSealAndStillMutates` (Go, shows R1 alone is insufficient) / `TestF2StorageComponentExclusivity.*` (Python, shows the static check that must catch it) | **FIRED AS PREDICTED; CONFIRMS THE STATIC CHECK IS LOAD-BEARING, NOT OPTIONAL** |
| **F3** | A struct whose interior is a closure over mutable state is not read-only just because the outer value was naively copied into `sealed[T]`; calling the closure still mutates captured state. | **R3:** `is_sealable(T)` recursively rejects `func`/`chan`/`interface`/pointer-shaped fields at seal time. | `TestF3_InteriorClosureLaundersMutationThroughNaiveSeal` (fires) / `TestF3Remedy_SealValueRejectsInteriorClosure` (holds) / `TestF3InteriorSealability.*` (Python) | **FIRED AS PREDICTED, REMEDY HOLDS** |

**Reading F2 correctly:** unlike F1 and F3, F2's remedy is not implementable inside the
runtime representation at all. `TestF2_PreSealSubsliceAliasSurvivesSealAndStillMutates`
is written to *fail* if a future runtime-only change accidentally "fixes" it — that
would mean the test stopped demonstrating the gap the static alias check exists to close.

## 3. Benchmarks (measured this run, `go test -bench=Benchmark -benchmem`)

AMD Ryzen 3 2200G, `go1.23.4 linux/amd64`, `GOMAXPROCS` default, `-count=1`:

```
BenchmarkSealMap10kNodes-4                  1000000000    0.2682 ns/op        0 B/op    0 allocs/op
BenchmarkCloneMap10kNodes-4                       2737   441106    ns/op   321869 B/op   11 allocs/op
BenchmarkSealSlice10kNodes-4                1000000000    0.2679 ns/op        0 B/op    0 allocs/op
BenchmarkCloneSlice10kNodes-4                   116649     9139    ns/op    81920 B/op    1 allocs/op
BenchmarkBroadcast8WorkersSealMove-4             51116    27083    ns/op      310 B/op    9 allocs/op
BenchmarkBroadcast8WorkersCloneBaseline-4          716  1759801    ns/op  2574692 B/op   97 allocs/op
```

- 10k-node map seal vs clone: **~1.6M× faster**, 0 B/op vs 321,869 B/op.
- 10k-node slice seal vs clone: **~34,000× faster**, 0 B/op vs 81,920 B/op.
- 8-worker broadcast (seal-once-share vs clone-per-worker): **~65× faster**, 310 B/op vs 2.57 MB/op.

These measured ratios are the honest numbers from this machine and this minimal
`map[int]int` / `[]int` shape — they are reported instead of the issue body's
pre-registered figures (which cite different hardware and a richer node/graph
shape) and should not be read as reproducing them exactly; the qualitative claim
(O(1)/zero-alloc sealing vs. O(N)/multi-alloc cloning) is what this spike verifies.

**Known gap:** this sandbox has no C compiler (`gcc`/`cc` not found), so
`go test -race` could not be executed here (`cgo: C compiler "gcc" not found`).
`TestP1_SealTransitionBroadcastsAcrossWorkersSenderStillReads` and
`TestP3_SealedForeignCallbackCaptureHasNoWritePath` exercise the same concurrent
read patterns the proposal's P1/P3 hypotheses depend on being race-free; running
them under `-race` on a host with a C toolchain is the natural next step before
this spike could support a stronger race-free claim.

## 4. Scope of This Spike

This spike verifies the *shape* of the proposal (runtime representation + the
admission rules' decision logic) at the level the Wildcard Track asks for —
"no working prototype required," a falsifiable idea with remedies. It does **not**
modify the real Rust checker (`compiler/crates/hy-check`) or emitter
(`compiler/crates/hy-emit`), does not add `seal`/`sealed` to the `.hy` grammar, and
does not touch `tests/checker-corpus/`. `checker/seal_checker.py` is a directly
executable model of the admission *decision logic* described in the proposal
(ownership / interior-purity / storage-component checks), not a parser or a
drop-in replacement for `hy-check`'s real alias/may_share dataflow.

## 5. Reproducing Verification Locally

```bash
python3 -m unittest discover -s spikes/w4843-seal-move-sealed-value/tests -v
cd spikes/w4843-seal-move-sealed-value/work
go vet ./...
go test -v -count=1 ./...
go test -bench=Benchmark -benchmem -run=^$ -count=1 ./...
```
