# W-012 current-main validation — 2026-10-23 UTC

Source tested: `0dfd117f86eab9e47789adc5ebe30b84d32a6462`, merging original PR #556 head `8a9e1d5` with actual main `7d63d5fc25ceb525c2d0f6ec89a4ad608a6a5011`. The documentation closeout preserves this tested compiler/analyzer/workflow source. [manifest.json](manifest.json) records exact source/tree, toolchains, binary hashes, commands, raw JSON hashes and environment failures. All measurements are new local samples, separate from the unchanged October 18/19 archives and from hosted CI.

## M1 gate and correctness

The live gate is **all 126 registered corpus programs** at this main revision: 38 ACCEPT, 2 ACCEPT-MANAGED, 86 REJECT. The issue's original 46 and older reports' 55/60/113 counts are historical checkout counts, not subsets used for current acceptance.

Both artifacts were independently built from this checkout with locked dependencies, Rust 1.99.0, two jobs and a worktree-specific Cargo target directory, serialized by `/tmp/heavy.lock`. Node 24.19.0 reports 126/126 expected verdicts and 126/126 complete native/WASM envelopes after filename normalization. The Go wrapper's separately modeled fields also agree on 126/126. The unchanged full `go test -race -count=1 -v ./...` and `go vet ./...` pass, including EOF positions, warning preservation/nonfatal vet policy, cache freshness, concurrent requests, active cancellation, traps and malformed envelopes.

An isolated Go consumer copied the analyzer module, loaded this precompiled WASM, built `hybvet`, observed HYB-E3003 on invalid input and passed after replacing only the `.hy` input. The cargo/rustc command names were poisoned throughout. This proves no invocation of those commands, not absence of a Rust installation or a shipped distribution.

The first WASM build failed because its standard-library target was not installed. After installing the pinned target, the unchanged build passed. The first full race run and explicit Go executable builds failed inside the filesystem sandbox: `go build -x` selected its injected outer `/workspace/.git` metadata and VCS stamping returned exit 128. Running unchanged commands outside that filesystem sandbox passed; no `-buildvcs=false`, wrapper, source modification or skipped regression was used. Full failed and passing local logs remain under `/workspace/go-hybrid-three-projects/pr-556`. The supplemental two-process `go run` cache experiment is kept there separately; the committed cache JSONs below use a normally built executable, matching the workflow.

## Current request-path measurements

Linux/amd64, Go 1.26.9, Wazero 1.9.0; five visible CPUs, GOMAXPROCS=2. This is one batch, one representative reject fixture, 25 copied files and 100 sequential fixed-width edits, with 10 warmups. The timer starts after each write and checks only the edited file. Each of the 100 records contains native, public and experimental guest timings; the execution order cyclically rotates all three modes. Cold initialization is separate. Nearest-rank medians were independently recomputed from the preserved samples.

| Request path | p50 ms | p95 ms | p99 ms |
| --- | ---: | ---: | ---: |
| Native subprocess + strict JSON decode | 1.742706 | 2.570464 | 3.706351 |
| Public `Engine.Check`, disposable guest | 2.766830 | 3.893633 | 6.005418 |
| Experimental sequential reused guest | 1.810732 | 2.389948 | 2.718908 |

The public path is slower on this host (about 1.59× native p50 latency). The experimental reused guest is also slightly slower than native at p50 and is not a proposed production pool. These data do not reproduce a public-path 18.7× speedup or sub-millisecond latency. No speed threshold was relaxed and no reruns were selected to improve the 100-edit result.

Public cold initialization in the 100-edit run was 601.347698 ms. Two later distinct Go processes sharing an initially empty optional compilation-cache directory recorded **573.906706 ms → 18.447271 ms**. These have only two request samples each and establish a cold compilation-cache observation, not hot analysis-result caching or a timing guarantee.

- [100-edit raw samples](w012-edit-benchmark.json)
- [First-process cache samples](w012-cold-cache-first.json)
- [Second-process cache samples](w012-cold-cache-second.json)

The native binary is 4,070,040 bytes, WASM is 1,714,571 bytes. This is an artifact-size observation, not CI image or package footprint. The raw two-file cache reports retain inherited limitation wording mentioning 25 files; their authoritative `Files` field is 2. Different hosts, compiler revisions and benchmark boundaries prohibit interpreting this and the older snapshots as a controlled trend.

M1 feasibility is supported within these boundaries. November 1 acceptance remains pending. Unsaved-buffer/gopls/LSP integration, live foreign Go type tables, packaged distribution, long-duration RSS, editor latency and CI image/onboarding footprint remain unimplemented or unmeasured. No local full Rust workspace run is claimed here; the normal compiler workflow is separate evidence.

## Hosted source checkpoint

Checks attached to published source `0dfd117` report **SUCCESS** for [quick and compiler](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39924581060), plus [analyzer-parity and Go-only consumer](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39924581079). The preflight check is SKIPPED, not passing. These are head-associated statuses; the hosted checkout/merge identity belongs to each run's checkout log and must not be silently relabeled as a bare-head execution. Current documentation-only closeout requires its own normal hosted status after publication; no new compiler/analyzer source is introduced.
