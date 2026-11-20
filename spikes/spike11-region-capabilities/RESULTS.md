# Spike 11 — Region capabilities for task-boundary ownership

## Registration (committed before analysis)

- **Target:** Go-Hybrid wildcard bounty #304, evaluated against main at `7a03aeb41b3ce29a0757bca2a5ddf22b9f61ef14`.
- **Question:** Can allocation-branded linear capabilities replace the existing per-binding alias/loan/freeze state while preserving current corpus behavior and avoiding frequent manual region annotations?
- **Hypothesis:** A region capability model can represent the existing mutable-sharing and task-boundary cases, but it does not remove value-flow analysis; it shifts that analysis to propagation of region sets through allocations, projections, calls, aggregates, closures, and control-flow joins. Unless those edges are inferable from current language evidence, the proposal's ergonomics and <250 LOC targets are not established.
- **Acceptance thresholds copied from bounty #304:** Encode at least 24 existing corpus cases under Region-Brand typing; falsify ergonomics if over 20% of idiomatic accept-corpus programs need explicit region wiring that cannot be inferred; falsify complexity if region-typing rules need over 250 LOC.
- **Method:** Read the current task-boundary rules and `hy-check` state transitions; inventory the existing corpus cases that exercise aliases/loans/freezes; map each needed relation to allocation identity, region propagation, or capability transfer; identify behaviors not inferable from current syntax alone. Do not change compiler behavior. No fuzzing or external targets.
- **Measurement limits:** A source-only model can expose missing semantics and estimate the minimum state/operations, but cannot prove actual LOC or ergonomics without an executable type-system prototype and an agreed encoding. Any numerical conclusion must distinguish measured counts from estimates.
- **Stop conditions:** Stop before implementation if the proposal is semantically under-specified, fails a current guarantee, or cannot be evaluated within the pinned toolchain/environment. Report the limitation rather than inventing pass/fail data.
- **Expected artifact:** This file updated with evidence and a verdict; no compiler source changes unless the analysis justifies a narrowly scoped prototype.

## Evidence and verdict — source-only feasibility pass

- **Baseline:** commit `7a03aeb41b3ce29a0757bca2a5ddf22b9f61ef14` (shallow clone of main at evaluation time).
- **Observed scale:** `freeze.rs` is 626 lines and `closures.rs` is 570 lines (1,196 combined), consistent with the proposal's approximate 1,200-line target. `model.rs` adds another 614 lines for bindings, shapes, and per-function state.
- **Corpus:** 45 registered cases, not 24. Relevant behaviors include aliases through function and generic returns (26, 31), shallow aggregate copies (29, 30), closure capture/local aliases (36, 39), wrapped projections/composites (40, 45), element-copy sharing (43), and positive controls (41, 44). Interprocedural loans, scope shadowing, loops, const writes, managed code, and channel boundaries also constrain compatibility.
- **Required capability propagation (inference from current semantics):** a binding would need a set of allocation regions; assignment copies that set, aggregate construction unions child sets, projections select or conservatively union regions, calls need return/alias summaries, closures carry captured regions, and control-flow joins merge sets. A task transfer consumes each region capability and every later access through any old alias must prove the capability is still held. This is still value-flow analysis, with region provenance as its abstract domain.
- **Unknown boundary:** calls into ordinary Go and unresolved/generic return provenance need an explicit rule (reject, descriptor, or managed escape). Without one, `Cap[R]` cannot establish that all aliases of a region were found.
- **Acceptance status:** the source-only pass does **not** encode 24 corpus cases under Region-Brand typing, does not measure the >20% manual-wiring threshold, and does not establish a <250 LOC implementation. Those are unproven, not passed. The proposal remains plausible as a type-system direction, but the evidence here does not justify deleting the current analysis or claiming the rewrite is simpler.
- **Validation limit:** the repo pins Rust 1.99.0; this host has Rust/Cargo 1.90.0. No Rust tests or compiler changes were run. BrownPants inventory ran successfully, but its current Rust support is inventory-only and contributed no Rust call-flow evidence.
- **Verdict:** **inconclusive; no production fix recommended from this pass.** The smallest next falsifiable artifact is a standalone region-set interpreter and explicit encodings for corpus 05, 20, 26, 31, 36, 37, 40, 43, and their positive controls 16, 23, 41, 44. Only then count manual annotations across at least 24 cases and measure implementation LOC. If external Go calls or generic return aliases require broad per-binding provenance tracking, record that as evidence that the model relocates rather than removes the core data-flow problem.

## Bounded prototype result

A dependency-free standalone capability model (`region_model.rs`) was added after the registration commit. It models globally distinct allocation regions, alias copies, aggregate unions and projections, linear transfer, fail-closed missing call summaries, and branch joins. The six tests pass with local `rustc 1.90.0` (`6 passed, 0 failed`). This is a rule-algebra prototype only; it does not parse `.hy`, implement the compiler checker, or encode the existing corpus.

This supports one narrow claim: region capabilities can express transfer and invalidate sender aliases. It also confirms the proposal still needs region provenance at aggregate, call-return, external-call, and control-flow boundaries. The experiment therefore does **not** meet the 24-case milestone or establish the integrated <250 LOC target, nor does it measure the >20% manual-wiring threshold.

The repository pins Rust 1.99.0; this host has Rust 1.90.0. The model compiled and passed on the available compiler, but is not validated on the pinned toolchain or in the workspace. `rustfmt` was unavailable, and installing the component failed because the host could not reach `static.rust-lang.org`.

## Manual corpus mapping (not an executable encoding)

Notation: `r1` is an inferred mutable allocation region; `In[r1,T]` carries that region; the current task may access it only while holding `Cap[r1]`. `Fresh` allocates a new region, `Alias` preserves a region set, `Union` combines nested sets, `Transfer` moves capabilities, and `Unknown` is rejected. The following 26 existing cases can be described in that vocabulary, but this table is not a runnable encoding and therefore does not satisfy the bounty's 24-case acceptance gate by itself.

| Corpus | Region/capability reading | Expected result |
|---|---|---|
| 01 | Shared map `In[r1]` reaches multiple tasks without moving the unique `Cap[r1]`. | Reject |
| 02 | Same shared map is explicitly marked `managed`; bypass is audited outside the guarantee. | Accept with managed lint |
| 03 | `&stats` carries `r1`; a pointer does not duplicate `Cap[r1]`. | Reject |
| 04 | Mutex does not create a second region capability; the current strict rule still rejects it. | Reject |
| 05 | `Transfer(r1)` to worker invalidates the sender's `Cap[r1]`; later sender use fails. | Reject |
| 15 | Each worker allocates a private `r_i`; results move back through the channel to one collector. | Accept |
| 16 | Each producer-loop batch is `Fresh(r_i)` and transferred once; no post-transfer access. | Accept |
| 17 | Scalar, immutable, atomic, and channel cases carry no mutable shared-region capability requiring transfer. | Accept |
| 19 | A read through `In[r1]` after `Transfer(r1)` fails for the same reason as a write. | Reject |
| 20 | Pre-existing `Alias(m2,m)` gives both bindings `r1`; the alias write fails after transfer. | Reject |
| 21 | `let` changes binding mutability, not the mutability of the map region `r1`. | Reject |
| 24 | Inner shadow `m:int` has no `r1`; outer `m:map` retains its separate region and still fails at spawn. | Reject |
| 25 | Spawned closure/callee carries captured region `r1`; it cannot be sent as a fresh closure. | Reject |
| 26 | Call summary `same(x) -> Alias(arg0)` preserves `r1`; returned alias use fails after transfer. | Reject |
| 27 | Package-level mutable storage is an ambient region; writes through `const`-lowered storage need an immutable-region rule. | Reject |
| 29 | Shallow struct copy unions the same child region `r1`; a non-owned parameter cannot transfer it. | Reject |
| 30 | Shallow array copy likewise retains child region `r1`; a non-owned parameter cannot transfer it. | Reject |
| 31 | Generic identity summary preserves the argument's region set; absent summary becomes `Unknown` and fails closed. | Reject |
| 36 | Closure capture carries `r1`; moving another alias does not leave the closure with access. | Reject |
| 38 | Closure alias of const-root storage retains the ambient region; writes need an immutable-region rule. | Reject |
| 39 | Closure-local alias of captured `m` retains `r1`; task transfer invalidates later access. | Reject |
| 40 | Nested call/projection/composite wrappers use `Alias`/`Union` to retain `r1`. | Reject |
| 41 | Alias reads occur before `Transfer(r1)` and stop afterward; the control remains valid. | Accept |
| 43 | `copy` of reference elements makes destination and source include the same child region `r1`. | Reject |
| 44 | Primitive element copy creates no shared child region; distinct slice allocations remain distinct. | Accept |
| 45 | New outer slice is `r2`, but `Union(r1,r2)` includes the shared nested map; transfer cannot duplicate `Cap[r1]`. | Reject |

The encoding immediately exposes required information not present in the 135-line model: allocation and field identities, closure capture summaries, generic/call return summaries, and aggregate region unions must be inferred from syntax and control flow. Case 37 (interprocedural map-slot invalidation) additionally belongs to the separate loan-window analysis and was not counted as a capability-only case.

## Latest-main recheck

The issue requires latest-main evidence. After this spike was first registered against `7a03aeb41b3ce29a0757bca2a5ddf22b9f61ef14`, upstream main advanced to `b73ce2fc485df6711b8e4cb21893fe547bd04878`. I rebased the spike branch onto that commit, confirmed the corpus still has 45 `.hy` fixtures and the mapped case descriptions remain present, and reran the standalone model: **6 passed, 0 failed**. The executable check is `rustc --edition=2021 --test spikes/spike11-region-capabilities/region_model.rs -o region_model_tests.exe` followed by running the binary. This is not the repository's pinned Rust 1.99.0 and is not a substitute for `make test-quick`.
