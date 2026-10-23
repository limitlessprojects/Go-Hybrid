# W-012 / issue #268 — the stock checker as an in-process WASI module: results

**Status:** gates pre-registered in [PREREG.md](PREREG.md) before the reported
run. Reported run: 2026-10-23, commit `18e17d4`, one host.
**Question:** W-012 wants the validator to stop being a subprocess. Is the
*cheap* endcap enough — the `hyb` binary we already ship, compiled to
`wasm32-wasip1`, run through `wazero` — or does the win require the
expensive endcap (a purpose-built engine ABI, or a self-hosted Go rewrite)?

**Short answer:** the boundary is genuinely ~3.7× cheaper in-process
(0.24 ms vs 0.90 ms to invoke and load), but that saving does not survive
contact with the check itself: median keystroke-to-diagnostic is a wash
(1.385 ms in-process vs 1.342 ms native), and the tail is 2.4× worse
(p99 7.214 ms vs 2.970 ms). H1 (semantics) passes at 126/126. H2 (sub-ms,
≥2× faster) and H4 (negligible cold start) are **falsified**; H3 (the
boundary is the cheaper boundary) passes. The bottleneck W-012 has to attack
is the checker body under WASM and per-check instance setup — not process
spawn.

## Build

One tree, no compiler change, no new crate:

```sh
(cd compiler && cargo build --release -p hyb)                        # native,  4,069,984 B
(cd compiler && cargo build --release -p hyb --target wasm32-wasip1) # WASI,    2,377,380 B
```

Rust 1.99.0 (the repository pin), Go 1.26, `github.com/tetratelabs/wazero`
v1.12.0 in the spike's own module.

## Method (as pre-registered)

- **Parity:** every `tests/checker-corpus/*.hy` (126 programs: 38 ACCEPT,
  2 ACCEPT-MANAGED, 86 REJECT), one invocation per engine, full-envelope
  identity — verdict, managed flag, and every diagnostic's code and position.
- **Latency:** a 25-file workspace copied out of the corpus; 100 sequential
  edits (a trailing comment line), one check after each edit; 3 repetitions
  per engine; engine order alternating each repetition. 300 samples per path.
- **Floor:** 30 invocations per engine against a missing input, so only the
  boundary (spawn, or instantiate + `_start`) is measured.
- Nearest-rank percentiles, no interpolation. WASI instance teardown is
  outside the timed window (documented in the harness).

## Results

Verdict parity — **126/126 both engines, 126/126 full-envelope identity**:

| | corpus | native matches | in-process matches | engines agree |
|---|---|---|---|---|
| all 126 programs | 126 | **126** | **126** | **126** |

Latency, 300 checks per path, milliseconds:

| engine | p50 | p90 | p99 | min | max | mean |
|---|---|---|---|---|---|---|
| native subprocess | **1.342** | **1.774** | **2.970** | 0.940 | 5.645 | 1.434 |
| in-process WASI | 1.385 | 2.981 | 7.214 | **0.287** | 9.254 | 1.680 |

Invocation floor, 30 probes per path, milliseconds:

| engine | p50 | p90 | min | max |
|---|---|---|---|---|
| native subprocess | 0.895 | 1.003 | 0.814 | 1.577 |
| in-process WASI | **0.240** | 0.506 | 0.117 | 0.857 |

Per-repetition medians (no repetition contradicts the pooled picture):
native 1.328 / 1.368 / 1.328 ms; in-process 1.427 / 1.388 / 1.288 ms.

Cold start: **one-time module compile 982.84 ms** for a 2.4 MB module
(no wazero compile cache in this harness).

## Gate verdicts (fixed in PREREG.md, not revisited)

| id | gate | measured | verdict |
|----|------|----------|---------|
| **H1** | 126/126 verdict agreement + full-envelope identity for both engines | 126/126 and 126/126 identity | **accepted** |
| **H2** | in-process p50 ≤ 1 ms *and* ≤ 0.5× native p50 | 1.385 ms; 0.97× native | **falsified** |
| **H3** | in-process invocation floor ≤ 0.5× native floor | 0.240 ms vs 0.895 ms = 0.27× | **accepted** |
| **H4** | one-time module compile ≤ 100 ms (falsifier at > 500 ms) | 982.84 ms | **falsified** |

## What this means for W-012

1. **The boundary is not the problem, and it is not the win either.** Spawn
   costs ~0.90 ms here; WASI instantiation ~0.24 ms. Deleting the process
   buys ~0.65 ms per check — real, but less than a tenth of the 15–40 ms the
   proposal attributes to process creation, on Linux. The proposal's 35 ms
   baseline does not reproduce on this host: the native path already
   answers in ~1.3 ms.
2. **The WASM body is slower than the native body.** Subtracting the floors
   (indicative, not a clean phase split): ~0.45 ms of check in native versus
   ~1.15 ms in-process — the interpretive cost of the same checker compiled
   to WASM plus WASI syscalls eats the spawn saving and more. A boundary
   swap alone therefore cannot reach "under 1 ms"; the body has to change
   (self-hosted Go) or the instance has to stay warm.
3. **The tail, not the median, is what an editor feels.** In-process p99 is
   7.2 ms against 3.0 ms native: one keystroke in a hundred pays more than
   the whole native path's 99th percentile, and the minimum shows the
   in-process path *can* answer in 0.29 ms. Variance, not central tendency,
   is the thing to fix: warm instances, bounded GC, and a compile cache.
4. **A stock WASI command module is not "zero-install".** ~1 s of module
   compile precedes the first diagnostic unless the embedder caches the
   compiled module (wazero's `CompileModule` cache) or keeps a daemon. Any
   M2 blueprint that claims instant first-run feedback has to name that
   cache as part of the design.
5. **Semantics are safe to move.** 126/126 identity, including diagnostic
   positions, means the WASI boundary does not change what the checker says.
   The migration risk in W-012 is performance, not correctness — which is
   the opposite of what the proposal's mitigations assume.

Consequence for the M1 milestone: a *parity* gate is achievable with the
cheap endcap today. The *latency* gate as written ("under 1 ms", "vs 35 ms")
is falsified by measurement on the stock binary; if M1 is to pass on
latency, it needs the warm-instance ABI (the expensive endcap) — and even
then the evidence here says the remaining gain comes from keeping state
warm, not from deleting the process.

## Honest limits

- One host, 4 CPUs, warm page cache, full machine to the benchmark; no
  editor, no `gopls`, no concurrent packages. Absolute numbers will move on
  other hardware; the ratio direction is what this run supports.
- Files are corpus-sized (tens of lines). Bigger packages shift both paths,
  probably in favour of in-process, which is why the pre-registered gates —
  not a projection — decide.
- The floor probe measures a path that fails early on a missing file, so
  "body = full − floor" is indicative only: it includes file I/O for a real
  input and none for the probe.
- No compile cache, no warm instance, no ABI tuning: this is the naive
  cheap-endcap configuration, deliberately, because that is what "no new
  crate" buys.
- Cross-check against PR #556 (custom `hy-wasm` cdylib, 2026-10-18) points
  the same way from a different mechanism: it measured 2.143 ms in-process
  vs 1.204 ms native, and also declined to claim the 18.7× speedup.

## Raw artifact

`report-2026-10-23.json` — the full report: every parity row
(per-program expectation, native envelope, in-process envelope), pooled and
per-repetition statistics, floors, machine string, commit.
