# W012 M1 evidence: current validation and historical snapshots

## Current delivery checkpoint (2026-10-23 UTC)

PR #556 originally pointed to `8a9e1d5546d96d6734ff4a8dc3213f81eca91280`. This repair merges actual main `7d63d5fc25ceb525c2d0f6ec89a4ad608a6a5011`, retaining the original history and every main STATUS line. Only STATUS conflicted. The [2026-10-23 same-checkout validation and raw samples](spikes/w012-inprocess/validation-2026-10-23/README.md) test source `0dfd117f86eab9e47789adc5ebe30b84d32a6462`: 126/126 expected corpus verdicts, 126/126 complete native/WASM envelopes and 126/126 public Go modeled-field parity; full Go race/vet and Go-only consumer pass. Earlier completed hosted runs below remain evidence for their historical trees; current head-associated [quick/compiler](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39924581060) and [W012 parity/Go consumer](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39924581079) checks also report SUCCESS. Hosted checkout identity is distinct from bare-head local evidence. M1 review remains scheduled for November 1; acceptance, merge and award remain maintainer decisions.

### Request-path interpretation

| Evidence | Timed boundary | What the result supports |
| --- | --- | --- |
| Original 0.269 ms / 18.7× prototype | Already instantiated guest versus native subprocess; earlier host and harness | Historical experimental result only; no current public-path speedup |
| 2026-10-18 raw 100-sample snapshot | Public `Engine.Check` 2.143 ms p50 versus native subprocess plus strict JSON decoding 1.204 ms; reused guest 1.097 ms | Public path was about 1.78× the native latency on that host |
| Archived 2026-10-19 25-file/100-edit proxy | Public 2.096 ms p50 versus native 1.150 ms; reused guest 1.451 ms; timer starts after writing, only the changed file checked | Public path was slower; no whole-workspace or editor latency conclusion |
| Current source `0dfd117`, local 2026-10-23 proxy | Public 2.766830 ms p50 versus native 1.742706 ms; experimental reuse 1.810732 ms | Public path is slower on this host; no editor, packaging or long-duration RSS result |

The current implementation compiles WASM once, then `Engine.Check` installs a 30-second context and `WasmEngine.CheckSource` instantiates and closes a disposable guest for every request. The separate sequential reused-guest control omits that lifetime cost. Compilation caches affect cold module compilation, not guest-per-request lifetime. Source changes, different hosts, cancellation support and changed validation prevent interpreting the snapshots as a controlled regression trend. New measurements must retain native/WASM hashes and raw samples; neither a faster control nor old CI can substitute for testing this exact integrated source.

**Live M1 correctness gate:** all **126 registered corpus programs** at main `7d63d5f` (38 ACCEPT / 2 ACCEPT-MANAGED / 86 REJECT), rather than a subset satisfying the issue's original 46-fixture wording. Historical 55/60/113 counts and 100-sample timings below retain their original source labels. Current full Go race passed using unchanged commands outside the filesystem sandbox after a bounded probe isolated its VCS-stamping failure; the failed local run is retained, and no buildvcs override or source workaround was used. The current two-process cache observation is 573.906706 ms → 18.447271 ms, isolated from hot request timings.

**Related independent evidence:** [PR #1295](https://github.com/limitlessprojects/Go-Hybrid/pull/1295), by masterdobra-hash, investigates stock `hyb` compiled as a WASI command, avoiding this spike's custom ABI. Its author explicitly contributes evidence to W-012 without claiming its slot or a milestone. This report acknowledges that adjacent approach; its parity and timings are author-reported independent measurements, not checks rerun here or measurements of this custom-ABI path. A maintainer can compare ABI maintenance cost against WASI command compatibility without conflating their hosts, runtime versions, teardown boundaries or implementations.

The archived JSON files are unchanged. The earlier warning-policy caveat in the PR description and 2026-10-18 snapshot is historical: the current vet adapter preserves structured warnings in `Engine.Check` but reports only errors through `analysis.Diagnostic`, so accepted warning-only source does not fail vet.

## Historical reviewer decision brief (2026-10-19 JST)

**Status: W-012 is scheduled for the 2026-11-01 M1 review. It is not yet M1-awarded or merged.** This is an executable feasibility and falsification record for embedding the existing Rust checker, not a production toolchain replacement or completed IDE integration.

### Completed integration with actual main

Integration commit `4149882c3f8cc5c636d559ec93cbb2f416098c65` merges branch head `0d1dbf711f7fb7c3147ca320c9435e86a8a6b0b1` with actual upstream `main`, `aa4a9d91b5edecd2bff37eba23fdd7befe60bd56`. Only `STATUS.md` conflicted; its upstream status was retained with a scoped W-012 note. The EOF fix, raw measurement archive, WASM project, and all upstream changes are preserved.

Both repositories completed fresh validation of this integration:

| CI repository | W012 validation | Compiler CI | Expected verdicts / complete native-WASM envelopes |
| --- | --- | --- | --- |
| Upstream | [39213961382 — PASS](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39213961382) | [39213961322 — PASS](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39213961322) | **113/113 / 113/113** |
| Contributor fork | [39213960125 — PASS](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39213960125) | [39213960076 — PASS](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39213960076) | **113/113 / 113/113** |

The actual upstream checkout was merge `1e8596ab3d3b5e561176f1d2e5b10242dbfeb512`; the fork checkout was merge `824347060429dc587758ec1540a1a32c1438e05f`. Both complete trees are **`9a27169f0bd8de82069b2a948730369b7f3182fd`**, identical to published head `4149882`. Each W012 workflow independently rebuilt native `hyb` and WASM from its own checkout before comparing the entire normalized diagnostic envelopes. Each also passed the complete `go test -race -count=1 ./...` suite, `go vet ./...`, gofmt enforcement, and its separate Go-only consumer, whose log confirms `cargo` and `rustc` were not invoked. Compiler tests, corpus verdicts, differential agreement, runtime hygiene, and examples passed in both compiler workflows. The EOF regressions are included in the full analyzer suite; its nonverbose logs report package results rather than individual test names.

**Current upstream status (2026-10-19 07:13 UTC):** PR #556 reports `mergeable_state=clean`, `mergeable=true`, head `4149882`, and base `aa4a9d91`; the actual main ref remains `aa4a9d91`. The integration conflict and missing new-head upstream validation are resolved. The PR remains open and M1 remains pending for November 1. The earlier HTTP 403 limits editing the upstream PR description; it does not describe the result of these successful code-validation workflows. This completion update changes only `PLAN.md`, `STATUS.md`, and this report; the tested code, tests, workflows, and archived benchmark bytes remain unchanged.

**Correction to the previous base claim (2026-10-19 06:54 UTC):** `f6e4d929` was the SHA reported by PR #556's `base` metadata, not the live `main` branch. Git transport and both upstream ref and branch APIs independently identify `main` as `aa4a9d91`. Comparing `0d1dbf7` with that actual ref reports 35 commits ahead and **251 behind**, with `f6e4d929` as their merge base. The earlier `behind_by=0` comparison was valid only against `f6e4d929`; it did not establish integration with live main. The PR still reported `dirty`, and its merge ref still pointed to historical `b49e0606`. The upstream repository can read the current head object, and the PR's 46 changed-file records match the comparison against its merge base, so neither an inaccessible head nor a different patch explains the discrepancy.

### Completed validation of the earlier recorded trees

Intermediate commit `61d5bd9b0655a7f6e638f010cab6093358aa6f51` joined the EOF fix `fd4f084` with `f6e4d929`. Its [W012 validation](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39210700789) and [compiler CI](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39210708370) passed. Both checker artifacts were rebuilt from that checkout: **61/61** expected corpus verdicts and complete normalized native/WASM envelopes, `go test -race -count=1 ./...`, `go vet ./...`, compiler differential agreement, and the separate Go-only consumer all passed. Executed merge `75b89aa59aed286c141dd8c87e2238c31f5d24e6` and `61d5bd9` share tree `880e3aee3dfc92dd10893e0d92f09439224707a4`.

The later documentation head `0d1dbf7` also passed [W012 validation](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39211484374) and [compiler CI](https://github.com/1932819331hy-debug/Go-Hybrid/actions/runs/39211484338), with the same **61/61** correctness gates and a Go-only consumer that did not invoke `cargo` or `rustc`. Executed merge `0e39fd07cf1047f3dce5110f6c974dda9ab3808e` and published `0d1dbf7` share tree `7e44239def4ef5fe829c83359638da226a961cd9`. These results and the local **60/60** and earlier hosted **113/113** results below each belong to their stated checkout. The new integration's 113/113 result comes from its own fresh builds and runs above; it does not reuse the older merge's result or artifacts.

### Latest correctness fix: preserve real EOF diagnostics

The Hybrid lexer reports EOF on the empty final line after a newline. Go's ordinary token line table omits that line, so an unterminated block ending in LF or CRLF previously returned an analyzer infrastructure error instead of its real `HYB-E0002` diagnostic. Empty source likewise had no usable EOF position.

The analyzer now reserves one byte only in its token position table. The checker still receives the original source, and coordinate validation still uses the original content length. The new regressions verify LF, CRLF, repeated trailing newlines, the no-final-newline control, Unicode scalar-to-byte conversion, empty source, and rejection of positions beyond EOF.

**Executed local validation:** Go 1.26.9, Rust 1.99.0 and Node 24.19.0, using source `6dafd4973114fe35c77133db89c19828a0ad07b6` plus the EOF fix. Both native and WASM checker artifacts were built from that same checkout.

- The unmodified analyzer fails four new EOF cases; the no-final-newline and Unicode controls pass.
- The fixed analyzer passes all six cases.
- `go test -race -count=1 ./...` and `go vet ./...` pass.
- `node test_wasm_parity.js` reports **60/60** corpus verdict agreement and **60/60** complete normalized native/WASM envelopes for this branch checkout.
- `gofmt` and whitespace checks are clean.

The branch's 60-fixture checkout is different from the 113-fixture synthetic merge used in earlier hosted CI. The earlier [validation run 39195170609](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39195170609) tested merge `b49e0606be4746d90cb5399ebdb04166024676b5`, not the bare branch head. Mixing a WASM artifact from that merge with a native binary from the branch produces invalid parity evidence because their compiler trees differ. Only the same-checkout results above establish this fix's local correctness; new hosted checks remain separate evidence.

### Performance evidence preserved for M1 review

The [committed raw measurement archive](spikes/w012-inprocess/validation-2026-10-19/README.md) preserves byte-identical JSONs from run 39195170609, with checked ZIP/file SHA-256 values and separate source, merge and base identities. The timing values displayed here remain that historical snapshot; later CI measurements have not replaced or relabeled the archived data. Original attachments expire October 26, before the November 1 review; the repository archive preserves them, and subsequent request/cache attachments now retain data for 30 days.

| Question | Observed evidence | Boundary |
| --- | --- | --- |
| Can the Rust checker run in-process? | Wazero executes the existing WASM checker; same-checkout native/WASM parity passes | A ready-to-install WASM distribution is not shipped |
| Is the public request path faster? | Preserved 25-file/100-edit snapshot: native **1.150 ms** p50; public `Engine.Check` **2.096 ms**; experimental sequential reuse **1.451 ms** | Public WASM was slower on that host; this is an after-write changed-file request proxy, not editor or whole-package latency |
| Does a compilation cache help cold startup? | Two distinct Go processes recorded **395.981 ms → 12.217 ms** with matching artifacts | This does not improve hot checks or cache analysis verdicts |
| Can Go run a precompiled checker without invoking Rust? | The separate Go-consumer job in run 39195170609 passed with cargo/rustc command names deliberately blocked | The hosted VM may still contain Rust; no installer or CI-image-footprint result is claimed |
| Are source diagnostics preserved? | Structured Engine results preserve warnings; the vet adapter reports errors only, including actual EOF positions | Stock gopls and unsaved `.hy` buffer integration remain unimplemented |

**M1 design decision:** keep one Rust semantic implementation with a WASM adapter and retain native CLI for measured batch performance. The remaining product questions are editor integration, distribution, broad workloads, memory/RSS and CI footprint. Historical 18.7× and sub-millisecond prototype figures below must not be cited as performance of the current public path.


---

## Earlier maintainer decision memo (archival snapshot, 2026-10-19 JST)

**Scope:** An executable M1 feasibility spike for [W-012 / #268](https://github.com/limitlessprojects/Go-Hybrid/issues/268), not a request to replace `hyb`, claim a stock-`gopls` plugin, or merge a production IDE feature. This answers solicitation #5 by investigating the *delivery/execution boundary*; W-011 independently attacks the `.hy`/Go syntax surface. The two approaches are adjacent and can compose.

**Latest source/evidence:** [PR #556](https://github.com/limitlessprojects/Go-Hybrid/pull/556), commit `7b4d8c35e7349dba532084ae1555412ce6d314a7`; [W-012 validation run 39171041567](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39171041567) **PASS**; [compiler CI 39171041397](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39171041397) **PASS**. The validation log reports **113/113 corpus expected verdict agreements**, **113/113 full native/WASM normalized JSON-envelope agreements**, `go test -race -count=1 ./...`, and `go vet ./...` successful. Each fixture count belongs to that tested checkout, not an unbounded future corpus.

**Execution/correctness hardening:** Rust's authoritative `hy-check` is compiled as a WASM guest and invoked from a Go `analysis.Analyzer` via wazero. Caller cancellation and an internal 30-second timeout fail closed; trap, malformed envelope and guest-memory errors cannot silently produce an accepted verdict. Anonymous module instances make concurrent checks on the shared runtime viable (the named-instance failure was reproduced on the rescue branch). Managed warning diagnostics remain nonfatal to `go vet`. Unicode-scalar positions convert to Go byte offsets and invalid source coordinates fail closed. Input discovery is deterministic; unreadable declared `.hy` inputs cannot be skipped silently.

**Cache/invalidation boundary:** Stock `go vet -vettool` does not track sibling `.hy` bytes as action-cache dependencies. The supported `hybvet vet` wrapper therefore uses `go vet -a` plus a pre-run SHA-256 workspace snapshot, verifies the actual bytes at each declared/scanned `.hy` read, then verifies the snapshot again after vet. A dedicated regression tests valid→invalid→valid edits, a second `.hy` file, mutated/deleted/added files and stale snapshots. The conservative `-a` choice favors correctness over production-speed cache performance.

**Falsification and cost — three maintainer questions:**

1. **WASM/Wazero versus a native-Go checker rewrite:** The embedded WASM reuses one existing semantic engine instead of maintaining a second borrow-checker implementation. Portability and single-source semantics are plausible gains. This does not by itself prove a no-Rust *build* or no-binary runtime distribution; a compiled WASM artifact still has to be supplied.
2. **Latency:** The historical `0.269 ms / 18.7×` benchmark measured a pre-instantiated experimental guest path, not today's public `Engine.Check`. The separate 100-sample Linux snapshot for the corrected public wrapper measured p50 **2.143 ms** versus native subprocess+JSON p50 **1.204 ms**, i.e. the implemented wrapper was **slower** on that host. Latest head and real IDE keypress-to-diagnostic latency have not been rebenchmarked. Do not infer sub-millisecond user-visible checks or an end-to-end speedup.
3. **Fail-closed input/diagnostic contract:** Proven at the WASM ABI and on-disk Go analyzer boundary for the tested corpus and injected failure classes. A stock `gopls` instance does **not** dynamically load third-party analyzers or open `.hy` unsaved buffers. `pass.TypesInfo` supplies Go's *structural types*, not Hybrid's `Produce/Borrow/Mutate/Consume` ownership effects; typed descriptors and an editor-specific LSP/host are still needed.

**M2 acceptance/failure experiment to run next (not yet claimed):** benchmark 100 sequential *real source edits* in a 25-file workspace against a native CLI and a deliberately specified in-process request path on the **same host**, reporting cold startup separately from warm p50/p95/p99, cancellation, memory/RSS and pre/post artifact sizes. Explicitly include CI onboarding/image-footprint cost with and without the Rust toolchain. If the only correct public path remains slower, publish that as a negative result and recommend retaining the native CLI for batch checks while limiting WASM to embedding use cases where deployment characteristics, rather than latency, justify it.

**Review disposition requested:** independent **M1 feasibility verdict** on the measured tradeoff, not a claim of achieved `gopls`, browser playground or a $250 award. The report below retains historical observations unchanged for provenance.

---

## Earlier hosted validation (2026-10-18 UTC / 2026-10-19 JST)

The [latest hosted M1 parity workflow](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39171466042), against PR head `585dac8922a481124fcacd3cb2424f05c8b8e7e2` merged with its then-current base, passed **113/113 checker-corpus fixture agreements** and **113/113 complete normalized native/WASM JSON-envelope comparisons**. Its `go test -race -count=1 ./...` and `go vet ./...` steps succeeded. The [independent compiler CI](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39171466089) passed as well.

This latest head fails closed on unreadable declared `.hy` files rather than silently skipping them; it also canonicalizes file names to prevent duplicate diagnostic reports and emits diagnostics in deterministic path order. Regression tests for the read-failure, path-alias, and ordering cases ran in hosted CI. The 113-fixture count describes this specific tested merge, not future upstream corpus revisions.

**Prior hosted snapshot:** [run 39143438366](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39143438366) passed 79/79 at `fef92eecf31ee1c3fe383a2953e17ea1840b9ff0`.

That head also adds a **30-second fail-closed limit to the public `Engine.Check`** and a cancellable `CheckContext` for embedding hosts. Context-log snapshots copy all slice backing arrays, and cancellation, subsequent runtime reuse, and snapshot isolation have regression tests. Hosted green CI establishes the tested behavior, not unsaved-buffer, LSP, or interactive editor integration.

## Earlier 60-fixture correctness and benchmark snapshot (retained for provenance)

The tested correction at `34c759f91e4ff76c8b6d587208cbf4bea6a51ca0` fixes a real Go-wrapper loss of accepted managed-safety warnings. On that older **60-fixture** checkout, raw native/WASM envelope parity was **60/60**; Go wrapper modeled-field parity improved from **58/60** to **60/60**, with the race suite passing.

The original 100-sample cloud measurements put the **actual public `Engine.Check` at p50 2.143 ms / p99 7.176 ms**, versus native subprocess + JSON validation at **1.204 / 1.796 ms**. The reused-instance experimental boundary was **1.097 / 1.521 ms** and is not the analyzer's actual per-call instance strategy. These results establish neither an 18.7× speedup nor sub-millisecond IDE latency. The more recent timeout change was **not rebenchmarked** here; different hosts and harnesses prevent treating these figures as a controlled performance regression comparison.

See the [measurement record, commands, limitations, and raw samples](spikes/w012-inprocess/validation-2026-10-18/README.md). Its 60-fixture and timing numbers are a historical snapshot, not the latest hosted acceptance run.

## Historical review snapshot follows

The original narrative below is retained for provenance. Its 55-fixture, 903.4 KB, 0.269 ms, 18.7×, and amortization figures describe the earlier recorded baseline only. They must not be quoted as measurements of the current corrected wrapper. Its architectural recommendations are hypotheses/tradeoffs rather than proof of completed editor integration.

# W-012 M1 Technical Due Diligence & Confirmatory Architectural Audit
## In-Process Analysis Driver for Go-Hybrid: WASM/Wazero vs. Native Go vs. Subprocess Architecture

- **Subject**: Issue [#268](https://github.com/limitlessprojects/go-hybrid/issues/268) — W-012 Wildcard Proposal
- **Evaluation Target**: Milestone 1 (M1) Feasibility & Proof-of-Concept Confirmatory Due Diligence
- **Base Upstream Commit**: `e2a5f7d5eb90becb210df274765adfa50c99b21b` (`upstream/main`, Session-15)
- **Local Branch**: `spike/w012-inprocess-analyzer`
- **Preregistration Commit**: `58ad41ae78f240680525a634aad8309fc9ad7281`
- **Protocol Class**: Preregistered Confirmatory Replication (N=100 iterations)
- **Status**: **STATUS: PUBLIC EVIDENCE — AWAITING M1 REVIEW**
- **Review Date**: 2026-10-18

---

## 1. Executive Verdict: CONDITIONAL GO (GO FOR REVIEW)

- **Verdict**: **CONDITIONAL GO (GO FOR REVIEW)** (Pivot Architecture A to Persistent Embedded WASM / Standalone LSP Server; Terminate Architecture B Native Go Rewrite; Reject stock `gopls` plugin claims; Submit to 2026-11-01 M1 Review Window).
- **Core Summary**:
  1. **Architecture A (Rust -> wasm32-unknown-unknown -> Wazero)** is **TECHNICALLY PROVEN**:
     - Compiles with 0 code modifications into a standalone **903.4 KB** (`925,098` bytes) WASM binary.
     - Achieves **100.0% verdict parity across all 55 corpus fixtures** (39 REJECT, 2 ACCEPT-MANAGED, 14 ACCEPT).
     - Achieves steady-state in-process check latency of **0.269 ms (p50)** and **0.278 ms (mean)**, delivering a representative **18.7x speedup** (≈ 17–19x) over native CLI process invocation (5.038 ms p50, 5.122 ms mean across 100 runs).
     - No state drift was observed during the tested horizon across 2,000 sequential and 3,200 concurrent runs. Long-duration memory behavior remains unproven.
  2. **Architecture B (Self-hosted Native Go Rewrite)** is a **HARD NO-GO**:
     - Rewriting `hy-check` (a large multi-thousand-line semantic rewrite; `hy-check` core alone is 16,663 LOC across 16 modules of Polonius-style alias tracking, non-lexical loan windows, and shadow scoping) creates permanent semantic divergence, duplicate maintenance overhead, and violates the project's zero-duplication invariants.
  3. **Proposal Flaws Corrected & Hardened**:
     - **Stock `gopls` Plugin Impossibility**: `analysis.Analyzer` can read non-Go files as auxiliary inputs, but stock `gopls` does not parse `.hy` into Go AST/types nor dynamically load arbitrary third-party analyzers.
     - **Baseline Subprocess Latency Corrected**: Issue #268's original estimate of 15–40 ms baseline subprocess latency was **falsified/overestimated** on this host (measured p50: 5.038 ms). However, in-process WASM (0.269 ms) retains a decisive 18.7x steady-state speedup.
     - **Cold-Start Amortization & Break-Even**: Wazero cold JIT compilation takes **346.119 ms (p50)** without cache. However, Wazero's persistent filesystem cache (`NewCompilationCacheWithDir`) slashes compilation to **12.919 ms (p50)**, achieving break-even against the native CLI in **under 3 checks**.
     - **Go `types.Info` Boundary**: `types.Info` can eliminate a meaningful subset of structural foreign-type stubbing (package whitelists, named type existence, method signatures, scalar atomic basic-type discrimination), but **cannot** infer Hybrid linear ownership, loan windows, or mutation effects (`Produce`, `Borrow`, `Mutate`, `Consume`). An ABI descriptor table remains mandatory.
     - **Safety Invariant Enforced**: Diagnostic rejection (`Verdict::Reject`) and infrastructural execution failure (`Verdict::AnalysisFailure`) are strictly decoupled. Infrastructure failures fail closed and never masquerade as user code rejections.

---

## 2. Scope of What Was Actually Proven

To maintain absolute maintainer credibility, this audit explicitly segregates verified empirical proof from partial findings and disproven assumptions.

### PROVEN
1. **Rust Checker WASM Target**: `hy-syntax`, `hy-check`, and `hy-diag` compile directly to `wasm32-unknown-unknown` without OS dependencies.
2. **Wazero In-Process Embedding**: Pure Go host embedding via `github.com/tetratelabs/wazero v1.9.0` executes without CGO or external toolchains.
3. **Corpus Verdict Parity**: 55/55 corpus fixtures match as normalized parsed JSON envelopes (input filenames normalized only) against the native CLI across all diagnostic codes and verdicts.
4. **Warm Latency Advantage**: Pure in-process WASM checking executes in **0.269 ms (p50)** vs **5.038 ms (p50)** for native CLI subprocesses (18.7x speedup).
5. **Thread and State Isolation**: No state drift or obvious memory-growth signal was observed during the 5,200-call test horizon in Rust and Wazero.
6. **Failure-Domain / Fail-Closed Separation**: Fault injection tests (WASM trap, malformed JSON, invalid memory bounds, alloc failure, context timeout) deterministically trigger `AnalysisFailure` and fail closed, never returning `Accept`.
7. **Filesystem Compilation Caching**: Wazero filesystem cache drops compilation time from 346.119 ms to 12.919 ms (p50), breaking even in 3 checks.
8. **On-Disk Singlechecker / `go vet` Integration**: Sibling `.hy` files are discovered via disk scan, and exact source spans map to `token.FileSet`, outputting exact `line:column` positions in `go vet`.

### PARTIALLY PROVEN
1. **Go `analysis.Analyzer` Integration for Sibling `.hy`**:
   - Proven: A real `analysis.Analyzer` can discover sibling `.hy` files on disk, register them in `pass.Fset` via `AddFile` + `SetLinesForContent`, and report exact `line:col` diagnostics.
   - Limitation: The documented contract of `pass.ReadFile` only permits files listed in `OtherFiles`, `IgnoredFiles`, or `Files`. Stock Go toolchains omit `.hy` from these sets. The spike demonstrates on-disk `singlechecker`/`go vet` integration, not virtualized/unsaved editor-buffer integration.
2. **Go Semantic Context Replacing Structural Stubs**:
   - Proven: `types.Info` can eliminate structural stubbing for imported packages, type names, and basic-type atomics.
   - Limitation: Cannot infer Hybrid linear ownership, loan windows, or consuming calls. Hybrid-specific semantic descriptors remain necessary.

### NOT PROVEN / DISPROVEN
1. **Stock `gopls` Dynamic Integration**: Disproven. Stock `gopls` has no mechanism to load external `analysis.Analyzer` binaries dynamically at runtime.
2. **Arbitrary Unsaved `.hy` Editor-Buffer Support**: Disproven under stock `go/analysis`. Non-Go files are not recognized as language documents in `gopls`.
3. **Pure Go Compiler Rewrite (Option B)**: Disproven as economically and architecturally unviable.

---

## 3. Real `analysis.Analyzer` Spike & Contract Audit

A minimal, real, runnable prototype was constructed in [`spikes/w012-analyzer/`](spikes/w012-analyzer/) adhering to `golang.org/x/tools/go/analysis`.

### The `pass.ReadFile` Contract vs. On-Disk Execution
The official Go documentation for `Pass.ReadFile` specifies:
> *"The only valid file names are the elements of OtherFiles and IgnoredFiles, and names returned by Fset.File(f.FileStart).Name() for each f in Files. Analyzers must use this function (if provided) instead of accessing the file system directly. This allows a driver to provide a virtualized file tree (including, for example, unsaved editor buffers) and to track dependencies precisely to avoid unnecessary recomputation."*

Empirical findings from [`analyzer_test.go`](spikes/w012-analyzer/analyzer_test.go):
1. **`pass.Files`**: Length strictly 1 (`sample.go`). Go parser ignores non-Go files.
2. **`pass.OtherFiles` & `pass.IgnoredFiles`**: Both are empty (`[]`). Non-Go/non-C files are not tracked by the Go toolchain.
3. **Execution Domain**:
   > *The current spike demonstrates on-disk singlechecker/go-vet integration, not virtualized/unsaved editor-buffer integration.*
   Sibling `.hy` discovery currently operates via directory scanning (`os.ReadDir`) using the directory of the Go package anchor.

### Source-Position Mapping
Initial spikes emitted diagnostics at `token.NoPos` (`-:`). By registering discovered `.hy` files into `pass.Fset` via:
```go
fileToken := pass.Fset.AddFile(filename, -1, len(content))
fileToken.SetLinesForContent([]byte(content))
```
Exact line and column positions from Hybrid diagnostics are translated to `token.Pos`:
```bash
$ go vet -vettool=./cmd/hybvet/hybvet ./testpkg
testpkg/sibling.hy:6:5: [HYB-E1002] `y` is declared and never used — Go refuses to build the emitted `y := ...` (declared and not used) (HYB-E1002)
testpkg/sibling.hy:7:5: [HYB-E1002] `z` is declared and never used — Go refuses to build the emitted `z := ...` (declared and not used) (HYB-E1002)
testpkg/sibling.hy:7:13: [HYB-E3003] `x` is read after ownership was moved away
exit status 1
```

- **Verdict**: **`Exact on-disk .hy positions: CONFIRMED`**.
- **Overall Contract Verdict**: **`analysis.Analyzer contract: PARTIAL FEASIBILITY`** (On-disk `go vet` passes fully verified; virtualized editor buffer streaming not supported by stock driver).

---

## 4. Corrected `gopls` Integration Analysis

Issue #268 asserted: *"Zero-Install IDE Diagnostics (<1ms vs 35ms) inside gopls by implementing an `analysis.Analyzer`"*. This conflated batch `go vet` passes with editor language server protocols.

### Definitive Technical Reality
> **Canonical Statement**:
> *analysis.Analyzer can read non-Go files as auxiliary inputs, but stock gopls does not parse `.hy` into Go AST/types nor dynamically load arbitrary third-party analyzers.*

### Detailed Technical Breakdown
1. **Dynamic Plugin Architecture**: Unlike tools supporting shared object plugins (`.so`), stock `gopls` compiles all enabled analyzers directly into its binary (`golang.org/x/tools/gopls/internal/analysis`). There is no runtime configuration or flag to dynamically load an out-of-tree `analysis.Analyzer`.
2. **Document & Syntax Boundary**: `gopls` is built around the Go Abstract Syntax Tree. It tracks buffers corresponding to `.go` file URIs. A `.hy` file opened in an editor is not recognized by `gopls` as a managed language document.
3. **Realistic Integration Pathways**:
   - **Path A: Upstream Analyzer into `gopls`**: Infeasible. The Go tools team does not upstream external DSL checkers into stock `gopls`.
   - **Path B: Custom `gopls` Fork**: Feasible for enterprise forks, but completely forfeits zero-install.
   - **Path C: Standalone LSP Server (`hyb lsp`)**: **The Proven Industry Standard**. Implementing an LSP server in Rust using `tower-lsp` or in Go wrapping Wazero. Editors route `.hy` files to `hyb lsp` and `.go` files to `gopls`.
   - **Path D: W-011 Directive-on-Go Migration**: If W-011 succeeds in expressing Hybrid rules as `//hy:` directives inside valid `.go` files, `analysis.Analyzer` becomes natively compatible with Go tooling.
   - **Path E: Editor Extension with In-Process Engine**: A VS Code extension embedding the 903 KB `hy_wasm.wasm` directly into Node.js/V8, providing <0.5 ms diagnostics with zero local daemon installation.

---

## 5. Corrected `types.Info` / `foreign.rs` Matrix

Go's `types.Info` provides rich lexical and semantic context (`Types`, `Defs`, `Uses`, `Selections`, `Scopes`, `Instances`, `Implicits`). However, Go's type system has **zero linear ownership or borrow concepts**.

### Structural vs. Hybrid-Specific Semantics
- **Category A (Structural Context - Solvable via Go Types)**:
  `TypesInfo` can eliminate a meaningful subset of structural foreign-type stubbing:
  - Hardcoded package whitelists (`BUILTIN_PACKAGES` in `foreign.rs`).
  - Named struct and interface type existence.
  - Method signatures and return types.
  - Scalar atomic discrimination: inspecting whether a `sync/atomic` type wraps a basic primitive vs an arbitrary pointer (`atomic.Pointer[T]`).
- **Category B (Hybrid Semantic Effects - Irreplaceable by Go Types)**:
  Go types cannot infer or derive Hybrid-specific ownership semantics:
  - `CallMode::Produce` (creates fresh owned handle).
  - `CallMode::Borrow` (initiates an A2 borrow loan window).
  - `CallMode::Mutate` (conflicts with live borrow loans).
  - `CallMode::Consume` (Rule A7 ownership transfer moving receiver away, e.g. `finish()`).
  - Handle roles (`OwnHandle` vs `BorrowHandle` element view).
  - Fail-closed security boundaries for unproven types (Rule FC, GUARANTEES row 11).

- **Conclusion**: `TypesInfo can eliminate a meaningful subset of structural foreign-type stubbing, but Hybrid-specific ownership/effect descriptors remain necessary.` These descriptors should be formalized as an external schema (`interfaces/descriptors.json` per SPEC OQ8).

---

## 6. Confirmatory 100-Iteration Benchmark Ledger

Measurements conducted on Apple Silicon under Darwin x86_64 emulation / arm64 runtime, running **100 iterations per case** against fixture `08-a3-map-slot-invalidated.hy`:

### Confirmatory Benchmark Table (N=100 Iterations)

| Execution Case | Phase | Min | p50 (Primary) | p90 (Primary) | p99 | Mean (Secondary) |
|---|---|---|---|---|---|---|
| **Baseline: Native CLI Subproc** | Fork + Exec + Check | 4.721 ms | **5.038 ms** | **5.476 ms** | 5.935 ms | 5.122 ms |
| **Case A: Fresh Runtime (No Cache)** | Module Compilation | 339.287 ms | **346.119 ms** | **371.130 ms** | 417.249 ms | 353.878 ms |
| | Module Instantiation | 0.170 ms | 0.210 ms | 0.270 ms | 0.340 ms | 0.216 ms |
| | First Check Execution | 0.700 ms | 0.760 ms | 0.880 ms | 1.120 ms | 0.772 ms |
| | **Case A Total Latency** | 340.200 ms | **347.100 ms** | **372.300 ms** | 418.700 ms | 354.967 ms |
| **Case B: Reused CompiledModule** | Module Instantiation | 0.060 ms | 0.075 ms | 0.110 ms | 0.140 ms | 0.078 ms |
| | Check Execution | 0.430 ms | 0.610 ms | 0.830 ms | 1.250 ms | 0.640 ms |
| | **Case B Total Latency** | 0.513 ms | **0.694 ms** | **0.947 ms** | 1.392 ms | 0.723 ms |
| **Case C: In-Memory Cache** | Cached Compilation | 9.583 ms | **9.887 ms** | **10.356 ms** | 21.417 ms | 13.384 ms |
| | Module Instantiation | 0.180 ms | 0.230 ms | 0.280 ms | 0.320 ms | 0.237 ms |
| | Check Execution | 0.710 ms | 0.780 ms | 0.890 ms | 1.020 ms | 0.803 ms |
| | **Case C Total Latency** | 10.500 ms | **10.900 ms** | **11.500 ms** | 22.800 ms | 14.437 ms |
| **Case D: Filesystem Cache** | Persistent FS Compilation | 12.337 ms | **12.919 ms** | **13.400 ms** | 14.090 ms | 12.974 ms |
| | Module Instantiation | 0.120 ms | 0.160 ms | 0.200 ms | 0.250 ms | 0.172 ms |
| | Check Execution | 0.710 ms | 0.890 ms | 0.990 ms | 1.130 ms | 0.923 ms |
| | **Case D Total Latency** | 13.200 ms | **13.980 ms** | **14.600 ms** | 15.500 ms | 14.356 ms |
| **Case E: Pure Warm In-Process** | Pre-instantiated Check | 0.260 ms | **0.269 ms** | **0.282 ms** | 0.740 ms | 0.278 ms |

---

## 7. Break-Even Quantitative Analysis

> [!NOTE]
> **Latency Baseline Finding**: Original 15–40ms normal CLI latency estimate was overstated on this host; real native CLI subprocess latency executes at **5.038 ms p50** (mean: **5.122 ms**).
> Representative steady-state speedup is **≈ 17–19x** (**18.7x** observed at p50).

### Metric Comparison
- **Native CLI Subprocess Latency**:
  - $T_{\text{subprocess\_p50}}$: **5.038 ms**
  - $T_{\text{subprocess\_mean}}$: **5.122 ms**
- **Pure Warm In-Process WASM Latency**:
  - $T_{\text{warm\_wasm\_p50}}$: **0.269 ms**
  - $T_{\text{warm\_wasm\_mean}}$: **0.278 ms**
- **Representative Speedup (p50-based)**:
  $$\text{Speedup}_{\text{p50}} = \frac{5.038\text{ ms}}{0.269\text{ ms}} \approx \mathbf{18.7\times}$$
  *(Mean-derived ratio: $5.122 / 0.278 \approx \mathbf{18.4\times}$)*.
- **Latency Advantage per Check ($\Delta T$ p50)**:
  $$\Delta T = 5.038 - 0.269 = \mathbf{4.769\text{ ms saved per check}}$$

### Calculated Break-Even Thresholds (p50-Derived)
1. **Cold Process without Cache (Case A)**:
   $$N_{\text{break-even\_cold\_p50}} = \frac{346.119\text{ ms}}{4.769\text{ ms}} \approx 72.6 \longrightarrow \mathbf{73\text{ checks}}$$
   *(Outlier-sensitive mean estimate: $353.878 / 4.844 \approx \mathbf{74\text{ checks}}$)*.
   - For single-file one-shot vet passes without cache, native CLI remains faster due to JIT compilation overhead.
2. **Process with Filesystem Cache (Case D, `NewCompilationCacheWithDir`)**:
   $$N_{\text{break-even\_fs\_p50}} = \frac{12.919\text{ ms}}{4.769\text{ ms}} \approx 2.71 \longrightarrow \mathbf{3\text{ checks}}$$
   *(Mean estimate: $12.974 / 4.844 \approx \mathbf{3\text{ checks}}$)*.
   - With persistent filesystem compilation caching, in-process checking breaks even on the **3rd file check**.
3. **Long-Lived Service / LSP Server (Case B / E)**:
   - Compilation cost is paid once at startup. The recorded pre-hardening warm microbenchmark measured **0.269 ms** and **18.7x**. This is not an end-to-end keystroke/save measurement.

---

## 8. Failure-Domain / Fail-Closed Model

```go
type AnalysisVerdict string

const (
    VerdictAccept          AnalysisVerdict = "Accept"
    VerdictReject          AnalysisVerdict = "Reject"
    VerdictAnalysisFailure AnalysisVerdict = "AnalysisFailure"
)

type AnalysisResult struct {
    Verdict     AnalysisVerdict
    Diagnostics []Diagnostic // Populated ONLY on VerdictReject
    Failure     error        // Populated ONLY on VerdictAnalysisFailure
    Summary     *Summary
}
```

### Safety Invariant
$$\text{VerdictAnalysisFailure} \cap (\text{VerdictAccept} \cup \text{VerdictReject}) = \emptyset$$

### Confirmatory Fault Injection Suite Results

| Injection Scenario | Injected Condition | Expected Verdict | Observed Verdict | Status |
|---|---|---|---|---|
| **Positive Control** | Valid program (`09-a3-map-slot-valid.hy`) | `VerdictAccept` | `VerdictAccept` | **PASSED** |
| **Negative Control** | A3 violation (`08-a3-map-slot-invalidated.hy`)| `VerdictReject` | `VerdictReject` | **PASSED** |
| **Fault 1: WASM Trap** | Real minimal guest `unreachable` trap (plus original mock) | `VerdictAnalysisFailure` | `VerdictAnalysisFailure` | **PASSED** |
| **Fault 2: Malformed JSON**| Truncated / corrupt diagnostic payload | `VerdictAnalysisFailure` | `VerdictAnalysisFailure` | **PASSED** |
| **Fault 3: Invalid Pointer**| Out-of-bounds guest memory pointer (50000) | `VerdictAnalysisFailure` | `VerdictAnalysisFailure` | **PASSED** |
| **Fault 4: Alloc Failure** | Mock allocator error returned at ABI boundary | `VerdictAnalysisFailure` | `VerdictAnalysisFailure` | **PASSED** |
| **Fault 5: Timeout** | Pre-expired context plus real infinite-loop guest interrupted during execution | `VerdictAnalysisFailure` | `VerdictAnalysisFailure` | **PASSED** |

- **Status**: **Safety fail-closed CONFIRMED; diagnostic/error-domain separation implemented.**

---

## 9. W-011 Dependency Matrix

| Capability | Without W-011 (Standalone `.hy` Surface) | With W-011 (Go Syntax Surface) |
|---|---|---|
| **Browser WASM Playground** | **Fully Functional**. Zero dependency on Go toolchain; compiles and runs `.hy` directly in V8/browser Monaco editor. | Fully Functional. |
| **Standalone LSP (`hyb lsp`)** | **Proposed architecture**. The embedded engine is demonstrated; no standalone LSP implementation or end-to-end editor latency is established here. | Redundant if gopls handles directives. |
| **`go vet` Integration** | **Partial Feasibility**. Discovers sibling `.hy` via directory scan anchor; emits exact `line:col` diagnostics. | **Native**. Runs directly on `pass.Files` with exact Go `token.Pos` source spans. |
| **Stock `gopls` Integration** | **Infeasible**. Stock `gopls` cannot parse `.hy` or load out-of-tree analyzer plugins. | **Infeasible dynamically**. Requires either upstreaming or custom gopls build. |
| **Custom `gopls` Build** | Requires hacking `.hy` grammar into gopls parser. | **Fully Functional**. Direct registration of analyzer on Go files. |
| **Live `TypesInfo` Integration** | Indirect. Requires companion Go package declaration for foreign stubs. | **Direct & Native**. Hybrid checker directly queries `pass.TypesInfo` of checked code. |
| **`.hy` Source Diagnostics** | Native byte-exact diagnostics via SPEC §9.6 envelope. | Obsoleted (diagnostics map to Go AST spans). |
| **Eliminate Foreign Stubs** | Partial. Eliminates package and scalar atomic lists. | **Complete**. All structural types inferred from active Go package. |

---

## 10. Updated Assumption Matrix

| # | Assumption in Issue #268 | Original Claim | Hardened Empirical Verdict | Final Evidence & Architecture Basis |
|---|---|---|---|---|
| 1 | Subprocess startup is a bottleneck | 15–40 ms latency | **PARTIALLY CONFIRMED** | Original absolute latency assumption (15–40 ms) was overstated (measured p50: 5.038 ms). However, relative in-process advantage is confirmed (18.7x speedup at 0.269 ms). |
| 2 | Rust checker compiles to WASM | Feasible | **CONFIRMED** | `wasm32-unknown-unknown` produces clean 903.4 KB binary with 0 OS dependencies. |
| 3 | Wazero simplifies deployment | High benefit | **PARTIALLY CONFIRMED** | Eliminates CGO and Rust toolchain requirements; requires compilation cache (12.919 ms) to avoid cold 350 ms penalty. |
| 4 | `TypesInfo` replaces `foreign.rs` | Fully replaces | **PARTIALLY CONFIRMED** | Eliminates structural stubs (packages, names, atomic types); cannot replace linear ownership or `CallMode::Consume/Borrow`. |
| 5 | `gopls` integration achievable | Easy plugin | **REJECTED** | Stock `gopls` has no dynamic plugin loader and ignores `.hy`. Must pivot to standalone LSP server. |
| 6 | `go vet` and `gopls` conflation | Distinct tools | **REJECTED (Conflated)** | Issue #268 incorrectly treated `analysis.Analyzer` as an in-process `gopls` plugin. |
| 7 | Fail-closed survives boundary | Safe | **CONFIRMED** | Hardened with explicit separation of `VerdictReject` vs `VerdictAnalysisFailure`. |
| 8 | Safe state persistence | No drift | **CONFIRMED** | 5,200 repeated passes in Rust and Wazero confirm zero state drift or obvious memory-growth signal. |
| 9 | Type transport simplicity | Simple | **REJECTED** | Serializing Go `types.Info` across WASM linear memory is complex; lightweight ABI descriptor table is far cleaner. |
| 10 | Self-hosted Go rewrite viable | Alternative | **REJECTED** | Multi-thousand-line rewrite (core `hy-check` is 16,663 LOC) creates permanent twin-compiler divergence. |

---

## 11. Final Architecture Recommendation

> **Architectural Conclusion**:
> *Architecture A remains viable for an embedded checker, go-vet/singlechecker tooling, standalone LSP/daemon, and browser WASM. The current Analyzer spike does not establish stock-gopls support for independent .hy buffers.*

1. **Ship Native CLI (`hyb check`)**: Remains canonical for one-shot batch builds and CI.
2. **Ship Standalone LSP Server (`hyb lsp`)**: Delivers sub-millisecond IDE diagnostics without hacking `gopls`.
3. **Ship Browser WASM Engine**: Leverage `hy_wasm.wasm` immediately for the online interactive playground.
4. **Shelve Option B (Native Go Rewrite)**: Permanently reject twin-compiler maintenance.
5. **Retain Spike as Reference**: Keep `spikes/w012-analyzer` as an unmerged reference harness pending future W-011 developments.

---

## 12. Submission Package Metrics & Sign-Off

- **Technical Review Readiness**: Evidence available for formal review; no numerical readiness or payout probability is inferred.
- **Actual M1 Payout Probability**: **UNKNOWN / Competition-Dependent** (17 competing entries for the $500 Hybrid Bounty pool; dependent on maintainer review priority in Discussion slot 12 on 2026-11-01).
- **Reviewed Upstream Commit**: `e2a5f7d5eb90becb210df274765adfa50c99b21b` (`upstream/main`)
- **Preregistration Commit**: `58ad41ae78f240680525a634aad8309fc9ad7281`
- **Global Status**: **STATUS: PUBLIC EVIDENCE — AWAITING M1 REVIEW**


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
