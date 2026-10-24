# W-012: 25-file, 100-edit request-path comparison

Purpose: independently measure the corrected public Go/Wazero request path against the native hyb CLI on the *same machine*, using real file modifications and the existing pre-registered corpus oracle.

**This is not a gopls or LSP benchmark.** It measures an on-disk single-file edit-to-check request proxy. It cannot establish an end-to-end keystroke-to-diagnostic time or successful integration of unsaved Hybrid buffers into stock gopls.

## Method

1. Create a temporary Go module with one ordinary Go anchor and 25 .hy source files copied from one pre-registered checker-corpus fixture.
2. Warm both check paths equally. Modify a different .hy file for every edit (round-robin through all 25), with 100 sequential, constant-width, non-cumulative edits. Start the request timer only after writing modified bytes.
3. Evaluate the changed file with (A) native hyb check PATH --json (new subprocess and JSON decoder each request), and (B) public Engine.Check(source) (in-process Wazero with already-compiled module and disposable guest per request). Alternate request order on odd and even edits.
4. Fail if the native exit code is inconsistent with a valid parsed envelope; if the registered expected verdict fails; or if modeled diagnostic codes, counts, severities, Tier-0 rules or source spans differ. Timing never overrides correctness.
5. Record all 200 durations plus p50/p90/p95/p99/mean, separate cold WASM compilation/init, SHA-256 hashes and sizes of the native and WASM artifacts, and basic host metadata.
6. CI saves the raw JSON as an upload-artifact attachment for the exact workflow run, with 30-day retention for request samples and cross-process cache timings. Performance figures must remain associated with their exact tested commit and host.

## Reproduction

From the root of the repository, first build both artifacts with the toolchain versions pinned in tools/env.sh:

    (cd compiler && cargo build --locked --release -p hyb)
    (cd compiler && cargo build --locked --release -p hy-wasm --target wasm32-unknown-unknown)
    (cd spikes/w012-analyzer && go run ./cmd/editbench -root ../.. -files 25 -edits 100 -warmup 10 -output /tmp/w012-edit-benchmark.json)

## Interpretation and falsification

The benchmark models only the request made *after* an edit. The native path must create a subprocess and read the file; Engine.Check receives source bytes in-process as an embedding host could. The paths are not identical user interfaces and no complete workspace reanalysis is performed. Cold compile time is isolated, never hidden in or misrepresented as warm request latency.

No assertion is made about editor startup, stock gopls, LSP, live type bridging, Docker image size, binary release packaging, or production RSS/long-duration memory use. The original M2 goal requires additional, independently verifiable experiments for these costs. The public Engine.Check may remain slower than the native subprocess; if so, report that falsification instead of substituting a different (faster) reused-instance prototype.


## Observed on hosted CI — 2026-10-19 JST

All runs built their own checker artifacts from the tested pull-request checkout; the raw samples are attached to each linked workflow run. These are different runs on hosted Linux workers, not a performance guarantee across machines.

| Validated checkout / run | Native p50 | Public Engine.Check p50 | Sequential reused-guest p50 | Interpretation |
| --- | ---: | ---: | ---: | --- |
| 7d0a942 / [39172638567](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39172638567) | 1.466 ms | 2.902 ms | not measured | Public WASM path slower |
| 9c04ee6 / [39173168608](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39173168608) | 1.458 ms | 2.648 ms | 1.789 ms | Reuse reduces guest creation overhead, but remains slower than native |
| 2e0a2b8 / [39174111506](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39174111506) | 1.485 ms | 2.613 ms | 1.792 ms | Earlier validated request-proxy sample favored native |
| 6719fa6 / [39175130478](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39175130478) | 1.493 ms | 2.620 ms | 1.799 ms | Earlier validated 25-file/100-edit sample: public path still slower |
| Source 6dafd497, tested merge b49e060 / [39195170609](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39195170609) | 1.150 ms | 2.096 ms | 1.451 ms | Preserved pre-EOF-fix hosted snapshot; public path still slower |

For the 9c04ee6 100-edit sample, the native subprocess finished faster in **all 100 paired trials** than both the public in-process path and the experimental reused guest. The experimental reused guest finished faster than the public in-process path in all 100 pairs. This is limited to one fixture, one Linux job and sequential execution, not a representative IDE workload.

**Cold WASM compilation cache:** The 2e0a2b8 run executed two *different Go processes* using the same optional HYBRID_W012_CACHE_DIR and matching WASM/native hashes. Public GetEngine cold initialization measured **616.613 ms for the first process** and **19.585 ms for the second**. This is a real cross-process compilation-cache benefit; it does **not** alter Engine.Check's hot request speed. The cache is opt-in; a directory controlled by the user must be protected from external modification. The control never caches source analysis results. The latest [6719fa6 run 39175130478](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39175130478) likewise recorded **623.789 ms → 20.105 ms** for a first versus second process with a protected shared compilation cache. This is a cold-start observation only.

**Go consumer without a Rust compiler invocation:** In [run 39174111506](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39174111506), a separate workflow job downloaded the precompiled WASM from the Rust-capable build job, compiled the Go hybvet executable with no Rust installation step, and executed failing-then-passing .hy source tests while cargo and rustc command names were explicitly poisoned. This verifies an execution boundary without *invoking* the Rust toolchain. It is not proof that the hosted VM contains no Rust installation, nor that a ready-to-install distribution has been shipped.

**Decision consequence:** Do not replace the production per-check disposable instance on these measurements. The proposed long-lived reused guest remains an isolated sequential research control; cancellation and concurrent ownership must be validated before any production pooling. For batch runs, native CLI is currently the faster measured path. A packaged Go host plus shipped WASM and a protected compilation cache is a plausible deployment benefit, but gopls integration, CI Docker footprint, broad-workload latency and RSS remain unproven.

## Durable review evidence

The [2026-10-19 archive](validation-2026-10-19/README.md) contains the byte-identical 100-edit JSON and both two-process compilation-cache JSONs from run 39195170609, with source/merge/base identities and verified artifact/file SHA-256 digests. The original attachments expire on October 26, before the November 1 M1 review. The committed archive remains available after that expiration. New workflow samples and cache timings retain artifacts for 30 days; this setting does not extend existing attachments.

These preserved measurements precede the EOF diagnostic-position fix. They are not a rebenchmark or correctness evidence for a later source revision. The EOF fix changes token-table mapping, not the checker input or the public Engine.Check request boundary; its correctness is verified separately by the analyzer regressions.


## Current-main local checkpoint — 2026-10-23

[Source `0dfd117` and preserved raw samples](validation-2026-10-23/README.md) rerun the same 25-file/100-edit after-write proxy on actual main `7d63d5f` plus this spike. Native p50/p95/p99: **1.742706 / 2.570464 / 3.706351 ms**; public `Engine.Check`: **2.766830 / 3.893633 / 6.005418 ms**; experimental sequential guest: **1.810732 / 2.389948 / 2.718908 ms**. The public path remains slower on this host. Two subsequent processes sharing the opt-in compilation cache measured **573.906706 → 18.447271 ms** cold initialization. These are new local samples, not replacements for the byte-preserved hosted archives or an editor/whole-workspace/production-memory claim. The current correctness gate covers all 126 registered fixtures, with fresh same-checkout artifacts.
