# Go-Hybrid — multi-agent worklog (append-only)

---
Task ID: 6 (session 5 — recovery + MVP-0)
Agent: lead (Super Z)
Task: recover crashed session's lost work, finish MVP-0 walking skeleton, continue plan

Work Log:
- Recovered state: cloned repo (PAT verified), read PLAN/STATUS/AGENTS + AST contract + corpus harness baseline (109 tests, 20/20 corpus, tripwire 10/15%)
- Re-pinned toolchains in fresh sandbox: Go 1.26.9 (/home/z/tools/go), Rust 1.99.0 (rustup minimal); background installs die silently in this sandbox — foreground curl+tar+rustup works
- Recreated the lost hy-emit crate from scratch (lib.rs context/file/fn assembly, stmt.rs, expr.rs, 35 tests): two-pass emission fixes recovered bug #1 (imports always before type decls), make() first-arg-as-type fixes bug #2
- Implemented SPEC E3 ?-desugar (unique hyErrN temps, named-result reuse) + E6 main exit contract (closure wrapper, defers-then-stderr-then-exit-1) + M4 normalization + binding hints (Foreign/Bytes) + interp→Sprintf (%% escaping) + task→go + move-marker drop
- Wired hyb build driver: version-pin enforcement (HYB-E9001, option_env pins + HYB_ALLOW_TOOLCHAIN_DRIFT hatch), parse→check→emit→go build
- Registry append-only +2 codes: HYB-E0002 parse-error, HYB-E9001 toolchain-mismatch (19 total)
- Completed example 03's lost struct decls (job/result) from the spike-6 twin
- tools/run_examples.sh + make examples + make test-quick (cargo test + corpus harness); Makefile recipe tabs fixed (spaces→tabs, prior session's logged failure mode)
- E2E verified: 01-hello exact match; 02-cli-cat happy path (bytes verbatim, exit 0) + error path (stderr "cat: open …: no such file or directory", exit 1); 03-worker-pool exact match [2 2 4 6 8 10 12 18]
- COMMITTED 9b2fe31 and pushed (MVP-0 walking skeleton milestone)

Stage Summary:
- Walking skeleton DONE: parse→check→emit→go build→run green on 01/02/03
- 144 tests green; corpus 20/20; tripwire not fired
- Both recovered emitter bugs have named regression tests (bug1 emission order, bug2 make type)
- Pending: version-drift script (PLAN), PLAN/STATUS refresh, batch B assessment (examples 04–09 need AST/parser extensions: inclusive ranges, break labels, select, generics/match, type aliases; 05/10 need the Rust bridge)

---
Task ID: 7-8 (session 5 — batches B/C)
Agent: lead (Super Z)
Task: version-drift script, batch B (example 04 surface), batch C (CI + docs + G1)

Work Log:
- tools/check-versions.sh: env.sh as single source of truth; cross-refs README/hyb defaults/PLAN; probes installed toolchains; --no-toolchain for CI; NEGATIVE-TESTED (injected drift caught); wired into make check
- Batch B (example 04): AST contract extension (lead-owned, logged): ForKind::Range.inclusive + Break/Continue.label; parser captures '..=' and 'break loop' (loop-keyword only — lexer skips newlines so bare-ident labels would misparse the next statement); emitter: hyLoop: labels + break hyLoop (loop is a Go keyword), select → Go case clauses 1:1, panic form
- Example 04 END-TO-END: total=204 == expected.txt (inclusive range correct: 1+4+9+...+64)
- run_examples.sh generalized (4 examples ALL PASS); emit tests 38 (select/inclusive/labeled-break/panic); 147 total; corpus 20/20 intact
- Batch C: CI compiler job (installs pinned toolchains from env.sh-discipline URLs, runs check-versions + cargo test + corpus + examples); gates/G1.md written (READY FOR HUMAN REVIEW); PLAN.md task list all-done through MVP-0 + 9 new Decision Log entries; STATUS.md rewritten; commits after EVERY batch per the no-data-loss directive

Stage Summary:
- Commits: 9b2fe31 (MVP-0 milestone) → 97285d3 (version script) → 8639e0b (Makefile tabs) → 1f8ad9e (example 04 + batch B) → batch C commit pending
- PLAN.md task queue now: enum/option/match + generics (07/08) then Phase-2 bridge (05/09/10)
- Sandbox lesson (repeat): background installs die silently; foreground curl+tar+rustup works

---
Task ID: 9 (session 5 — batch D + CI fix + docs)
Agent: lead (Super Z)
Task: programs 07/08 (Phase-1 exit), CI fix, README/docs refresh

Work Log:
- Batch D: option[T] prelude (val+ok struct, some() ctor, none=zero; return none lowers vs declared result); option-match → hyMatchN temp + .ok/.val if-else (guard = nested if); set[T] prelude (insert returns is-new); .pop() → hy_pop(&x); TypeAlias decl (type node_id = int — was a zero-field struct approximation); assoc fns mangle lru_cache.new → lru_cache_new; generic fns emit [k comparable, v any]; single unnamed results bare
- O8 map-slot loan desugar: x := &m[k] → copy-read + store-back at declaring block's end (Go cannot &m[k]; O9 honesty made operational; block-scoped loan_frames flushed via emit_block_body, ALL block sites routed through it)
- Chan binding hint: single-var iterate binds the VALUE — slices/maps emit '_, v', channels keep 'v' (hints from chan params + make)
- Parser: peek_generic_args accepts single type-shaped args (box[t]{v: 1}); ambiguity documented (ident[index](/{ favors generics; index-call shapes absent from surface)
- Example 07: touch() implemented per its own intent comment; Example 08: stack := []node_id{from} (bare-ident slice literal needs full local inference — deferred + logged)
- 157 tests (48 emit + 10 batch-D tests); corpus 20/20; examples 6/6 PASS
- CI failure diagnosed via API logs: runner rustup shim (no default toolchain) shadowed the fresh install → check-versions now tolerates empty versions + CI step prepends the installed toolchain PATH
- README 'Get it' rewritten (MVP-0 landed: real quickstart); PLAN/STATUS/worklog refreshed

Stage Summary:
- Phase-1 exit criteria MET (programs 1, 2, 7, 8 compile + match expected)
- Commits this session: 9b2fe31 → 97285d3 → 8639e0b → 1f8ad9e → 2bebc0c → 4000d4b → cfee21f (CI fix) → final docs commit
- Remaining plan work: Phase 2 (Rust bridge: programs 05/09/10 + call-overhead bench) — new phase, awaits G1 human review per the gate discipline

---
Task ID: 10 (session 5 — closeout)
Agent: lead (Super Z)
Task: CI green, final verification

Work Log:
- CI failure #2 diagnosed (examples step: hardcoded sandbox go path) → hyb build resolves go from PATH (HYB_GO_BIN override), HYB-E9001 still enforces the pin
- CI GREEN on 5431fd0: quick + compiler jobs (pinned toolchains, version pins, cargo test workspace, corpus 20/20, 6 examples end-to-end)
- Final local verification: 11/11 test suites ok, examples ALL PASS, working tree clean, all work pushed

Stage Summary:
- Session 5 COMPLETE. 9 commits pushed; every batch committed immediately (no data-loss window)
- State for the next session: G1 awaits human review (gates/G1.md); Phase 2 (Rust bridge: programs 05/09/10) is the next phase per PLAN.md

---
Task ID: 11 (session 6 — G1 ratification + Batches E/F/G)
Agent: lead (Super Z)
Task: review + ratify G1, then finish all pending phases (06, bridge 05/10, then 09/differential/bench)

Work Log:
- Fresh sandbox recovery: re-clone + re-pin Go 1.26.9 / Rust 1.99.0 (foreground installs); verified 157 tests, corpus 20/20, 6/6 examples
- G1 REVIEWED + RATIFIED per human directive: every lowering decision checked against T4/E3/E6/C1–C4/M4/O8–O9 + implementation; gates/G1.md header records provenance; commit 603e3e2
- Batch E (20e63a6): example 06 end-to-end — shapes.rs foreign shape table (field-vs-method per descriptor type), HandlerFunc conversion with target-typed closure params (ParamDesc interface-vs-pointer), E11 handler `?` contract (SPEC §6 new rule), httptest.get composite lowering (recorder dance), net/http import mapping, E6 wrapper terminal return, fprintf interp splice; 7/7 examples
- Batch F (3239154 + 783dc03 gitignore fix): Phase-2 bridge LIVE — runtime/rust/hyrt (sha3 0.10.8 + serde_json 1.0.140 extern C wrappers, panic=abort, handle table w/ generations, sentinel errors, two-leg buffer protocol), runtime/go/hybridge (spike-3 trampolines verbatim + Cross4), sha3binding (cgo address broker, pinned-borrow protocol, ABI handshake word), hyb build workspace assembly (module hybrid.program, cargo build hyrt w/ mtime freshness, CGO_ENABLED=1); CallArg.name AST contract ext (named args); example 10 end-to-end — digests verified vs Python hashlib, checksum deterministic; 8/8 examples
- Batch G (this commit): jsonbinding (handle Values + AddCleanup release + materialized Array slices), RUST_TYPES type mapping, E3b `?` expression form (SPEC §6 rule; hoist-to-binding w/ evaluation-order temps, fn-lit bodies excluded), iterate-var handle typing; example 05 end-to-end byte-exact; 9/9 examples, 181 tests, corpus 20/20
- Sandbox lessons: shell cwd resets between Bash calls (runner scripts in /home/z/my-project/scripts/), background installs die silently, gitignore caught a leaked target/ dir

Stage Summary:
- 9 of 10 target programs byte-exact; the plan's remaining work: program 09 (multi-package), differential loop, overhead bench, G2 doc
- Commits this session: 603e3e2 → 20e63a6 → 3239154 → 783dc03 → (G)

---
Task ID: 12 (session 6 — Batch H)
Agent: lead (Super Z)
Task: program 09 (multi-package Go-consumable emission) — the adoption story

Work Log:
- AST contract ext: FnDecl.is_pub (the parser previously parsed-and-DROPPED `pub` at parse_decl's top)
- hyb build --package <name> --out-dir <dir> --binding-prefix <path>: emits package <name> (pub-fn exports via M4) + hybridge core + per-crate bindings (imports rewritten to the consumer's prefix) + staticlib at <dir>/lib
- Emitter package mode: export_pub, configurable binding_prefix, E3-in-error-result-fn propagation (zero-results return) + `return _, err` zero-fill (SPEC M2b rule added)
- hyrt += regex =1.11.1 (is_match status-in-RAX 1/0/-1, is_valid bool — no error channel); regexbinding Go package (pinned-borrow strings, status-in-RAX)
- Example 09: program.hy (validate_email via the pinned email regex; simple_pattern = validity bool) + committed main.go.txt consumer — go run BYTE-EXACT (`ada@lovelace.dev <nil>` / `match: true`)
- Runner: 09 assembles the consumer module in the acceptance (skips the generic binary pre-build)
- 183 tests; 10/10 examples; corpus 20/20

Stage Summary:
- ALL TEN target programs byte-exact — the SPEC's ten-program set is fully green
- Remaining plan work: differential corpus loop, call-overhead bench (G2 evidence), G2 gate doc

---
Task ID: 13 (session 7 — Batch H commit + Batch I)
Agent: lead (Super Z)
Task: commit Batch H; then finish pending plan phases (differential loop → bench → G2)

Work Log:
- Session recovery: PLAN.md Batch H entry repaired (Example 06 text was concatenated into its reverse clause; duplicate Batch G entry removed); Batch H committed 37f165c + pushed — ALL TEN target programs byte-exact now in-repo
- Batch I (differential corpus loop): wrote tools/differential_loop.py — ACCEPT corpus programs must survive hyb build + run exit 0; REJECT programs re-verified checker-side; wired into make test-quick/corpus + CI
- The loop IMMEDIATELY caught seven real emission gaps on the 9 accepted programs (02/09/10/12/13/15/16/17/18): blank iterate (for _ := invalid Go), sync.mutex/atomic receiver typing, zero-result `?` hoist (bare hyT3), jsonbinding.Set scalar contract, short-return arity fill, composite const→var, type-elided map-value struct literals, pointer-param auto-address, hy-fn rust-typed result registration, duplicate hyErr assign
- All fixed in hy-emit (shapes.rs sync.mutex/sync.rwmutex/atomic.int64 entries + METHOD_RESULTS + RustFnDesc.results; lib.rs fn_shapes/struct_fields/var_struct/var_ty eager tables; stmt.rs shape-aware hoist_one_try + call_results + blank-iterate forms + emit_return fill; expr.rs type_of_expr extension + emit_expr_exp expected-type context); jsonbinding.Set(key, any) errors — scalars box via JSON Marshal/Parse round-trip inside the binding (zero bridge changes)
- MethodFunc variant added then REMOVED (dead — atomic.int64 is a struct-method surface) per the table's own discipline
- 11 new regression tests (one per fix, named); 194 tests total; differential 9/9 accept + 11/11 reject FULL AGREEMENT; 10/10 examples; corpus 20/20; tripwire not fired

Stage Summary:
- PLAN.md's differential row DONE; remaining: call-overhead bench (G2 evidence) + G2 gate doc
- Everything committed immediately per the no-data-loss directive

---
Task ID: 14 (session 7 — Batch J: overhead bench + G2)
Agent: lead (Super Z)
Task: call-overhead bench vs ≤5ns budget on the LIVE bridge + G2 gate doc

Work Log:
- hyrt += hy_nop (no-op extern — the boundary with zero payload); runtime/go/overheadbench (cgo broker per the sha3binding pattern, ABI handshake, Nop/NopRaw/DeepPathCount accessors)
- hyb bench-overhead subcommand: assembles the bench through build_with_bridge (the EXACT hyb workspace recipe), runs 3 process runs, reports the median; make bench target wired
- hybridge diagnostics: DeepPathCount (cold-path atomic — zero fast-path cost) + Cross1Raw (no-retry trampoline, documented diagnostic-only)
- DIAGNOSIS: the first run measured 17 ns — instrumented probe found the cause: a FRESH main-goroutine stack holds only ~2.9 KB headroom (< the fast path's 4096 B threshold; g.stackguard0 offset 16 verified against go1.26.9 DWARF via extract_offsets.py) → every shallow crossing engages the pooled deep path
- Two-scenario bench: shallow (deep path 19.19 ns, 15,100,002 crossings counted) vs grown-stack (fast path 3.37 ns ≤ 5 ns budget — PASS at 67%; raw trampoline 2.65 ns ≈ spike-1's 2.82 ns prototype; marginal delta 1.25 ns; deepcount frozen — path proven)
- gates/G2.md written + PASSED: budget holds on the fast path; the pool premium (~16 ns over the spike's fixed-stack prototype) documented as the recorded price of collision safety (spike-3's TestConcurrentCrossing finding); per-callee budgets logged as a Cut-Log follow-up
- Regression: 194 tests, 10/10 examples after the hybridge instrumentation

Stage Summary:
- The PLAN.md queue is now EMPTY of engineering items: programs 01–10 byte-exact, differential loop FULL AGREEMENT, G2 evidence + gate doc landed
- Next: human G2 review; G3 exit sweep (MVP exit criteria) + closeout docs

---
Task ID: 15 (session 7 — Batch K: G3 exit sweep)
Agent: lead (Super Z)
Task: G3 gate doc (MVP exit sweep) — the last plan item

Work Log:
- Verified PLAN.md has ZERO open task rows (every engineering item landed)
- gates/G3.md written: PROVISIONAL pending human review (per SPEC "don't idle at a gate") — claim-by-claim evidence links for all 7 SPEC MVP rows + performance + CI; the honest not-claimed list (option/match vs general enums, curated shape tables vs a real checker, sanitizers/fuzzing await the fleet dispatch, worker-tier bench pending); kill-criteria check: none fired
- PLAN gate row: G2 PASSED + G3 PROVISIONAL recorded; STATUS next-actions refreshed

Stage Summary:
- Session 7 total: Batch H committed (37f165c) → Batch I differential loop (dae5014) → Batch J bench + G2 (b34d157) → Batch K G3 sweep (this commit)
- The full plan chain is complete: 10/10 programs, differential FULL AGREEMENT, G2 PASSED, G3 evidence assembled — all pushed

---
Task ID: 16 (session 7 — closeout)
Agent: lead (Super Z)
Task: CI verification + session closeout

Work Log:
- CI failure diagnosed (differential step: relative --bin resolved inside the temp workspace cwd) → differential_loop.py absolutizes --bin up front + missing-binary guard; verified locally (not-found naming, relative-from-/tmp, default FULL AGREEMENT)
- CI GREEN on 43e06d3: pins → 194 tests → corpus 20/20 → differential 9/9+11/11 → 10/10 examples
- README Get-it refreshed to the final state (all ten programs, package emission, make differential/bench, gate statuses)
- Session commits: 37f165c (H) → dae5014 (I) → b34d157 (J) → 872ef1c (K) → f94ef89 (README) → 43e06d3 (CI fix) — every batch pushed immediately

Stage Summary:
- THE PLAN IS COMPLETE: PLAN.md task queue zero open rows; 10/10 programs byte-exact; differential FULL AGREEMENT; G2 PASSED; G3 evidence assembled (PROVISIONAL pending human review); CI green
- For the next session: human review of gates/G2.md + gates/G3.md; on approval the MVP may be called done; fleet dispatch (sanitizers/fuzzing) before any release

---
Task ID: 17 (session 8 — G2/G3 review + ratification)
Agent: lead (Super Z)
Task: review gates/G2.md + gates/G3.md per human delegation ("on approval the MVP may be called done then continue with rest of pending matters and through tests that you didnt cherry pick or device yourself just normal go and rust things")

Work Log:
- Fresh sandbox recovery: re-pinned go1.26.9 (foreground tar) + rust 1.99.0 (rustup minimal); core.filemode=false (the whole tree showed mode-flip noise, zero content)
- G2 evidence REPRODUCED via the bench: fast path 3.44 ns ≤ 5 ns (69%; gate's 3.37 within variance), raw trampoline 2.63 ns ≈ spike's 2.82, deep path 19.34 ns, DeepPathCount frozen across the fast loop — verdict unchanged
- G3 claims re-verified via STANDARD suites: cargo test --workspace 194/0 (11 suites), make examples 10/10, make differential 9/9+11/11, corpus 20/20 (tripwire quiet); go vet + go build clean on all five runtime packages assembled in the exact hyb workspace shape (module hybrid.program + staticlib at lib/)
- FINDING 1: make bench claimed at Batch J never landed (b34d157 touched no Makefile) → target wired
- FINDING 2 (material): Batch I's regeneration (dae5014) reintroduced space-indented recipes — EVERY make target broken ("missing separator") while CI stayed green because CI never invokes make → Makefile rewritten (tabs, deduped vars), make -n syntax guard in make check + CI Makefile-parse step
- FINDING 3: 5/9 runtime Go files not gofmt-clean → gofmt -w; vet/build re-run clean; 10/10 acceptance re-run after (no behavior change); gofmt+vet+build added as a CI leg
- FINDING 4: hyrt — unused Number import removed; documented crate-level allow for the 18 C-ABI pointer-deref lints (abi-types §5 protocol is the real contract); cargo fmt; clippy now clean
- Logged-not-repaired: compiler-workspace clippy = 33 style-level findings, 0 correctness lints → Post-MVP queue row 1 (churn at gate exit rejected)
- Post-MVP queue opened in PLAN.md (6 rows: style sweep, fleet sanitizers/fuzzing vs MVP SHA, worker-tier bench, per-callee budgets, package-level Go tests, make bench at fleet dispatch)
- Final sweep make test-quick green; commit 5deeea2 pushed (13 files); gates/G2.md + G3.md headers, PLAN (status/gate row/Decision Log/post-MVP queue), STATUS, README updated
- Sandbox lesson (repeat): shell env resets between Bash calls — export PATH inline every time

Stage Summary:
- GATE CHAIN COMPLETE: G0 passed / G1 ratified / G2 passed+ratified / G3 passed — THE MVP IS DONE, public claim unblocked
- 4 defects the gates' own claims missed were caught BECAUSE the review ran normal tooling — the user's instinct was right
- Next: post-MVP queue in order (row 1 style sweep is sandbox-local; rows 2–3 need the fleet)

---
Task ID: 18 (session 8 continued — post-MVP rows 1 + 5)
Agent: lead (Super Z)
Task: continue the post-MVP queue opened at the G3 review (style sweep, Go runtime tests; fleet rows blocked)

Work Log:
- ROW 1 (ec279ed): style sweep — cargo clippy --fix applied 33→21, remainder by hand: 15x doc-list overindent via scripts/fix_doc_overindent.py (drives cargo clippy --message-format=json, applies clippy's OWN spans/replacements — never hand-guessed), 2x loop→while-let, to_place→place_of (6 sites), FnShape type alias, callee shorthand, matches!; cargo fmt; clippy now 0 findings workspace-wide; full suite re-run (194/0, differential, corpus, examples all green)
- ROW 5 (3b6a6de): 20 Go tests across all five runtime packages vs the REAL staticlib — G2 path-proof + concurrency (overheadbench), SPHeadroom/constants canary (hybridge), the full jsonbinding handle protocol (move-zeroing, empty-key, type-strict getters, boxing, arrays), raw-\s regex lessons, NIST SHA3 vectors; CI hygiene leg extended to go test ./...
- Two tests corrected by OBSERVED contract (not silently): String(float) is type-strict (error, not coercion); serde_json Map = BTreeMap → sorted pretty keys (assertions order-safe under both orderings by construction)
- STALE-ARCHIVE TRAP documented: crate-local target/ held a pre-Batch-J libhyrt.a (missing hy_nop) because env.sh's shared CARGO_TARGET_DIR redirects manual builds; hyb's mtime check + CI are immune; rebuild crate-locally to refresh
- Fleet rows 2/3 assessed: NO gh CLI, no GITHUB_TOKEN in env, no Kaggle creds in this sandbox — dispatch impossible from here; the sanitizer/fuzz JOB SPEC composition is the next batch's sandbox-local work
- All work committed + pushed: 5deeea2 (review) → 8d09a30 (worklog) → ec279ed (row 1) → b2fbcc3-ish PLAN commits → 3b6a6de (row 5)

Stage Summary:
- Post-MVP queue: rows 1+5 DONE; rows 2/3 fleet-blocked (job-spec composition next); row 4 (per-callee budgets) needs its own design batch; row 6 rides the fleet
- Session state: MVP DONE + ratified; every standard suite green (cargo 194/0, clippy 0, go vet/test 20/20, gofmt clean, 10/10 examples, differential 9/9+11/11, corpus 20/20, bench 3.44 ns)

---
Task ID: 19 (Batch M)
Agent: Super Z (main)
Task: Land the customer-feedback fail-closed directive — checker fail-closed at critical boundaries (Finding 1), closure-body boundary checking (Finding 2), plus the parser crashes the new tests exposed; then Discussions/release/marketing rows into the queue.

Work Log:
- Sandbox rebuilt after crash: reinstalled pinned go1.26.9 (/home/z/tools/go) + rust 1.99.0 (rustup); Kaggle token saved to ~/.kaggle/access_token (chmod 600)
- Continued the in-flight fail-closed work: model.rs (Unknown out of is_sendable_by_type, Closure shape) + shape.rs closure-aware inference survived the crash; finished sendability.rs check_site (Outcome::Unproven → HYB-E3005 outside managed / E8001 escape inside, counted by --report-managed)
- Registered HYB-E3005 (unproven-at-boundary, rule FC) in hy-diag/data/diagnostics.json — 21 codes, fixits: ascribe (`var x T = ...`) / route through a known channel / managed hatch knowingly
- New closures.rs pass: task/send/select-send statements INSIDE fn() bodies are checked fail-closed with the closure's ascribed params + enclosing env as capture view; captures/loans/freeze stay outside (A5 documented) — corpus 10 stays pre-registered ACCEPT; managed blocks inside closures counted for O6.3 parity
- lib.rs/loans.rs: select-recv defines now use the channel's ELEMENT shape (ok = bool) instead of manufacturing Unknowns; local ascriptions fill inference gaps only (resolved inference keeps priority so boundary-handle loan windows stay precise)
- New tests (10 checker + 2 parser): unknown-initializer/unresolved-name/unproven-select-recv → E3005; managed escapes (function + closure) → E8001 + counts; ascription makes the share provable; closure send of shared map → E3002; closure unproven capture → E3005; corpus-10 shape unchanged; malformed select arm + brace mis-nest → clean E0002 (no-hang regression pins)
- CRASH-CLASS BUGS fixed: two parser infinite loops (a select arm starting `case x:` spun the arm loop via sync_stmt's zero-progress stmt-starter return; a brace mis-nest spun parse_file_body's decl loop via sync_decl's zero-progress `}` return) — force-progress invariant in both syncs; sync_stmt keeps `}` no-consume so `x := }` still recovers with ONE diagnostic
- Diagnostics verified end-to-end by hand (hyb check on repro files); all instrumentation removed; docs: SPEC-LANGUAGE §9.3 O4.9 + O4.10 written, §9.6 seed-set count 21; PLAN.md queue rows 7–10 (fail-closed DONE, release, Discussions, marketing) + Decision Log 2026-10-14 entry + Test Results baseline; STATUS.md rewritten

Stage Summary:
- cargo test --workspace 206/0 (was 194) · corpus 20/20 tripwire quiet · differential 9/9+11/11 FULL AGREEMENT · examples 10/10 · go vet+build+test clean · clippy 0 · fmt clean
- The Tier-0 contract is now sharp: everything accepted was proven; everything else is named, with the fix in the message
- Parser hangs: any malformed program now terminates with clean E0002s — a compiler that hangs was fail-open in the worst way

---
Task ID: 19-b
Agent: Super Z (main)
Task: Fix the CI differential flake exposed by the Batch M push.

Work Log:
- CI run 38455237116 failed: accept-side 02-shared-map-managed exited 2 with a Go concurrent-map-writes fatal — the program's OWN pre-registered header says "this exact program can still panic at runtime" (managed suppresses the checker rejection, not Go's runtime fatal). The run outcome was always scheduling luck; the earlier local "1 disagreement" I first saw was the same flake, not a Batch M regression.
- Made the honesty note machine-readable: corpus 02 header gains a `RUNTIME-HAZARD:` marker line; tools/differential_loop.py parses markers (HAZARD_RE) and accepts, for marked programs, a RUN-phase death by the documented Go fatal (build-phase failures still FAIL) — reporting it as "documented RUNTIME-HAZARD fatal (pre-registered)".
- 3 consecutive differential runs: FULL AGREEMENT (9/9 + 11/11).

Stage Summary:
- Differential is deterministic again; CI honesty preserved — the fatal is accepted ONLY because the corpus file itself pre-registers it.

---
Task ID: 19-c
Agent: Super Z (main)
Task: Fleet dispatch debugging — jobs 006/007 first attempt failed.

Work Log:
- Both fleet runs (heavy shards + kaggle) failed fast: (1) job.json sha field "v0.1.0" is a TAG — the runner's sha-check compares rev-parse HEAD against it → exit 1 (fail-soft, loud); (2) the build sentinel `hyb --help >/dev/null` broke every build chain — hyb exits 2 for usage/--help (the CLI's usage exit code, by design) → test -x replaces it.
- Fixed both job.json files (sha=HEAD like jobs 001-005; sentinel test -x), re-dispatching both tiers.

Stage Summary:
- Lesson recorded: hyb's usage exit is 2 — never use `hyb --help` as a success sentinel in fleet jobs.

---
Task ID: 19-d
Agent: Super Z (main)
Task: Fleet results — row 3 verdict recorded; fuzz leg path bug fixed.

Work Log:
- Kaggle job 007 (worker-tier G2 bench) SUCCEEDED: 4-core Xeon 2.20 GHz — fast path 6.93 ns > 5.00 ns budget (VERDICT: MISS, pre-registered budget), raw trampoline 5.36 ns, go-call floor 3.15 ns, marginal delta 3.78 ns, deep path 42.41 ns, newobject 888.5 ns; DeepPathCount froze at 15100001 — fast path proven taken. Sandbox PASS does not generalize to this tier (floor 1.5x slower AND marginal cost ~3x — hardware-sensitive stack-switch). Row 3 CLOSED with the honest MISS in PLAN.md; no code change implied.
- Heavy job 006: asan-bridge PASS + race-bridge PASS on the Actions shard (the sanitizers legs are GREEN on the fleet); fuzz leg failed in 0.1 s — reproduced locally on a fresh clone: hyfuzz ran check_one with cwd=temp-dir while --hyb was relative → FileNotFoundError before case 1. Fixed: abspath at startup. Verified: exact fleet command, 800/800 cases, all invariants held.

Stage Summary:
- Row 3: recorded MISS (the budget is sandbox-tier; row 4 per-callee budgets is the follow-up).
- Row 2: sanitizers legs proven on the fleet; fuzz leg re-dispatching after the one-line fix.

---
Task ID: 19-e
Agent: Super Z (main)
Task: Fleet closeout — row 2 green on both shards; session complete.

Work Log:
- Heavy job 006 re-run (dba0562): ALL FOUR LEGS GREEN on both shards — build-hyb 10.1 s, asan-bridge 21.0 s (4 bridge packages under ASan), race-bridge 13.1 s (5 packages under -race), fuzz-parser-checker 2.4 s (hyfuzz 800/800: no hangs, no crashes, no ICEs). Shard summary artifacts ok:true (fuzzer's own line captured: "800 cases, seed=20261014, clean-rejects=752, clean-accepts=48 — all invariants held").
- PLAN.md: row 2 CLOSED (fleet evidence recorded; nightly cargo-fuzz + native Rust sanitizers documented as the follow-up), row 3 CLOSED (worker-tier MISS 6.93 ns vs 5 ns budget recorded honestly; sandbox PASS does not generalize; row 4 per-callee budgets is the follow-up).

Stage Summary:
- Customer feedback batch fully landed: fail-closed checker (O4.9/O4.10) · two parser crash bugs fixed · v0.1.0 released with a smoke-tested artifact · Discussions + feedback thread live · README repositioned · sanitizers+fuzzing fleet-green vs the release SHA · worker-tier bench measured and recorded.

---
Task ID: 20 (Batch N)
Agent: Super Z (main)
Task: Merge Discussion #1's first reply into the plan + kill the install-friction trust blocker; reply to the community; post more questions.

Work Log:
- Fresh sandbox recovery: go1.26.9 + rust 1.99.0 re-pinned; clippy/rustfmt components added
- Discussion #1 reply analyzed (ssmurfgg04-gif, 4 asks: evidence, install, adoption framing, hatch hardening) — all four merged this batch
- GUARANTEES.md: line-by-line claims table over exactly the reply's primitive list (sync.Pool / atomic.Pointer / context.Context / finalizers / unsafe / cgo / channel-close), every row cites SPEC rule + corpus evidence, `no claim` where true, §9.2 quote-shape as normative header, soundness-bug protocol (P0, published severity/status, corpus regression before fix)
- --deny-managed landed in hyb check (policy gate: any managed block fails; JSON stdout purity kept — notice on stderr); 4 integration tests over corpus 02/15 (compiler/crates/hyb/tests/deny_managed.rs, CARGO_BIN_EXE + corpus paths with THREE ups from crates/hyb); hyb Cargo.toml gains serde_json dev-dep (envelope parse assertion)
- INSTALL FRICTION FINDING: v0.1.0's tarball shipped NO prebuilt staticlib — INSTALL.md told users to install Rust 1.99.0 (the exact pitch-death the reply predicted). Fixed: tools/package_release.sh (reproducible packaging: crate-local hyrt build for the mtime-freshness contract, hyb release, tarball+sha256, smoke test from the ACTUAL tarball with cargo REMOVED from PATH — PASS rustless); install.sh (one command: platform detect, latest-release resolve via API, sha256 verify, extract to ~/.hybrid, baked `hyb` launcher wiring HYB_RUNTIME_DIR, verify via `hyb explain E0002` — never `--help`, exit 2 by design); tools/RELEASE-INSTALL.md = the Rust-free install notes shipped in the tarball
- Repackaged + re-uploaded v0.1.0 assets (tarball now 7.9 MB with prebuilt libhyrt.a + .sha256 asset)
- PLAN.md: queue rows 11-15 opened (multi-platform matrix, Docker, sample-repo-vs-go.mod design, managed reason+categories phase 2, soundness ledger+bounty); Batch N Decision Log entry; fixed a concatenated Decision Log line (two 2026-10-14 entries on one line)
- README: Quick start leads with the one-command install (no Rust toolchain); guarantee section cites GUARANTEES.md + --deny-managed; -race wording audit: no parity claims found
- STATUS.md rewritten (community-review state)
- Suite: cargo test --workspace 210/0 (+4 deny-managed) · corpus 20/20 (tripwire quiet) · differential 9/9+11/11 FULL AGREEMENT · examples 10/10 · gofmt clean · go vet+build+test 5 pkgs ok · clippy 0 (--all-targets; fixed 2 pre-existing doc-list warnings in hy-syntax/tests/parse_all.rs) · fmt clean

Stage Summary:
- Both trust blockers the reviewer named are addressed with landed artifacts (claims table + rustless one-command install), not promises
- The tarball contract now: prebuilt staticlib wins ensure_staticlib's freshness check; packaging is a script, not an ad-hoc session
- Queue rows 11-15 carry the rest of the reply's asks (platform matrix needs the next fleet dispatch; go.mod sample repo + managed categories get design threads in Discussions first)

---
Task ID: 20-b (Batch N community ops)
Agent: Super Z (main)
Task: The discussion work — reply, react, more threads, poll, release-notes refresh.

Work Log:
- Discussion #1: point-by-point reply posted linking the landed artifacts (GUARANTEES.md / install.sh / --deny-managed / rows 11-15); the reviewer's comment heart-reacted
- New thread #2 (Ideas): platform priority — polls are NOT creatable via the GitHub API (CreateDiscussionInput rejects pollOptions) → reaction-voting pattern: one comment per option, vote by thumb
- New thread #3 (Q&A): the go.mod integration design question (toolexec vs go:generate vs make-step, constraints, current leaning = B with --check) — the reply's sharpest ask gets a public argument before code
- Release notes refreshed via PATCH (needs the release ID, not the `latest` alias): install section now leads with the one-command rustless installer; suite counts 206→210; What's-next points at the claims table + both threads
- Tooling: scripts/gh_disc.py (GraphQL reply/new/react — REST rejects discussion writes), gh_list_comments.py, gh_release_update.py (sandbox-local, not in repo)

Stage Summary:
- Community state: #1 feedback (answered point-by-point) · #2 platform poll (live) · #3 go.mod design (live) · release notes match the shipped tarball
- Lesson: GitHub Discussions writes are GraphQL-only; polls are UI-only; PATCH releases needs the numeric ID

---
Task ID: 21 (Batch O)
Agent: Super Z (main)
Task: Rows 11 + 6 + 15 — the release-matrix fleet dispatch, make-bench on the Actions tier, the soundness reporting path.

Work Log:
- release-matrix.yml (dispatch-only): matrix = linux-amd64 (native repro rebuild) · linux-arm64 (aarch64 cross gcc + qemu-user smoke) · darwin-arm64 (macos-14 runner, native) · windows-amd64 (mingw for cgo); every leg runs tools/package_release.sh — which refuses to ship anything that doesn't smoke-test RUSTLESS from the ACTUAL tarball; publish job uploads tarball+sha256 clobber to the v0.1.0 release
- package_release.sh cross-mode: HYB_RUST_TARGET/HYB_CC/HYB_PKG_OS/HYB_PKG_ARCH envs; arm64 uname mapping added; CGO_ENABLED=1 mirrored into the cross smoke (go defaults cgo OFF for foreign GOARCH — found while wiring, hyb sets it internally but the smoke's env must survive go's foreign-arch default)
- Soundness path (row 15): ISSUE_TEMPLATE/soundness.yml (P0-by-definition framing per GUARANTEES.md: minimal .hy repro + expected/actual checker output + severity claim we re-derive and publish) + config.yml linking discussions 1/2/3; `soundness` label created (b60205)
- Row 6: bench job in the same workflow — make bench on the Actions 4-vCPU tier = the THIRD measured tier (sandbox 3.39 PASS / Kaggle worker 6.93 MISS / Actions = this run's number)
- Local validation before dispatch: all 3 YAMLs parsed, sh -n clean, native packaging re-run → smoke PASS (linux/amd64, rustless)
- Dispatched with publish=true at HEAD; results to be recorded in the next worklog entry

Stage Summary:
- The poll in #2 now has a concrete answer in flight: ALL FOUR listed platforms build+smoke on the fleet this run — the poll's value shifts from prioritization to RC-testing volunteers
- Windows is the honest-risk leg (asm trampolines + cgo staticlib under mingw never ran before) — fail-fast: false records whatever happens

---
Task ID: 22 (Batch P)
Agent: Super Z (main)
Task: Row 4 per-callee fast-path stack budgets (+ row 6 bench wiring + row 16 found by the fleet).

Work Log:
- Fleet (release-matrix) run 2→3: fixed BSD paste (macOS), aarch64 sysroot for qemu (-L — binfmt has none, even the CLI couldn't exec), windows bridge moved to x86_64-pc-windows-gnu (msvc emits hyrt.lib; the cgo contract is -lhyrt → libhyrt.a via mingw ld); bench verdict = recorded datapoint not red build. Run 3: bench ✅ (5.30 ns MISS recorded), darwin/arm64 ✅, windows/amd64 ✅, linux/amd64 ✅ — arm64 leg led to the REAL finding: the bridge asm is amd64-ONLY (call_amd64.s excludes itself on GOARCH=arm64) → row 16 (aarch64 trampoline port) opened; the arm64 tarball ships honestly as checker+emitter (bridge-free programs), smoke runs the arm64 hyb under qemu -L
- ROW 4 LANDED: three guard classes — SMALL (1024, Cross1S..4S, new asm, byte-identical discipline) / DEFAULT (4096, Cross1..4, UNTOUCHED — G2 evidence stays valid) / DEEP (CrossDeep1..4, the pool; CrossDeep2..4 wrappers added). Class ledger interfaces/descriptors.md grounds every curated fn
- THE CLASSIFICATION CAUGHT A LATENT SOUNDNESS GAP: serde_json recurses on value nesting (remaining_depth=128; measured ceiling: 127 parses, 128 fails — probe on the pinned 1.0.140); pre-row-4 the json Parse rode the default 4096-guard fast path — a 127-deep parse needs an order of magnitude more stack than the guard proves → latent overflow on shallow goroutines for deep inputs. Fixed by the ledger: parse/clone/drop/serialize = DEEP (7 fns); the fix predates any external report
- Diagnostic parity: Cross2/3/4 now count deep retries (were silent); DeepCrossCount added (the ALWAYS-deep route's oracle — DeepPathCount only sees fast-path retries)
- Tests: overheadbench/guardclass_test.go (the (1024,4096) disagreement window: SMALL fast + DEFAULT deep at the SAME headroom; grown-stack SMALL loop frozen); jsonbinding/deepnesting_test.go (serde ceiling documented as a test; 127-deep parse/serialize/release via the DEEP routes with DeepCrossCount proof; the constrained-goroutine stress). Go suite 5 pkgs ok · gofmt clean · examples 10/10 (05 byte-exact through DEEP routes) · corpus 20/20 · differential 9/9+11/11
- Docs: interfaces/descriptors.md (the class ledger + adding-a-fn protocol); G2.md follow-up status APPENDED (nothing rewritten); PLAN rows 4/6 closed, row 16 opened; Decision Log entry

Stage Summary:
- Row 4 = shipped: shallow-goroutine fan-in for tiny callees takes the ~3-5 ns fast path instead of the 19-42 ns deep path, with the guard-soundness argument now written down per fn
- The batch's headline is the PROCESS: writing the class ledger forced the serde_json recursion analysis that caught the latent overflow — the claims-table discipline (Discussion #1 ask 1) working as designed inside the repo too

---
Task ID: 23 (Batch P close-out — the release matrix landed)
Agent: Super Z (main)
Task: Fleet debugging to green + publish all four platform assets.

Work Log:
- Runs 2-6 debugging trail, each fix real and committed: (2) BSD paste / aarch64 sysroot / windows-gnu / bench-verdict-tolerance; (3) arm64 leg exposed the DEEPER fact — the bridge asm is amd64-ONLY (call_amd64.s excludes itself on GOARCH=arm64) → row 16 opened; arm64 smoke redefined honestly (bridge-free program, hyb under qemu -L); (4) qemu-user-static ships *-static names; (5) HYB-E9001's found-but-silent rustc under qemu (empty stdout) → the pin now treats empty output as ABSENT (the pin governs rustc WHEN USED; bridge-free builds legitimately have none); (6) ALL LEGS GREEN + publish SUCCESS
- Release now carries: linux-amd64 + darwin-arm64 + windows-amd64 (FULL: checker+emitter+bridge, rustless-smoked) + linux-arm64 (checker+emitter, honest per-asset cut note) — each with sha256
- Release notes rewritten: the platform table with per-asset status + the row-16 pointer
- Discussion #2 (the platform poll) answered: all four shipped; remaining value = RC-tester volunteers; aarch64 port prioritized by tester demand
- Local baseline through every change: cargo 210/0 · go 5 pkgs ok · examples 10/10 · corpus 20/20 · differential 9/9+11/11 · gofmt clean · packaging smoke rustless PASS

Stage Summary:
- Discussion #1's install-friction ask is now FULLY answered: one command, no Rust, four platforms, sha256-verified, honest about the arm64 bridge cut
- The release-matrix workflow is the reusable release recipe (row 6's bench rides every dispatch)

---
Task ID: 24 (Batch U: the bounty decision landed + Batch V: the emitter sweep)
Agent: Super Z (main)
Task: Owner's bounty decision to a live program; the last documented follow-up (row 13's emitter base-indent) closed; community radar via the bounty-hunter shape.

Work Log:
- Read the state first: rows 1-16 were already closed, the bounty DECISION was the one open escalation; this session's owner message supplied it
- Batch U: wrote BOUNTY.md ($500 pool, top 10, ladder $75/$60/$50 + $45x7 with the reasoning recorded: strict 30% for the top three forces 3rd below 4th with 4-10 equal, so the top-three share lands at ~37% and the owner keeps final say), severity tiers, pay-only-after-reproduction, CI-pass-before-review, safe harbor, 48h triage SLA, first-reporter duplicates rule, AI-assisted reports (impact over volume, spam disqualified), all-contributors credit, Hall of Fame seeded with ssmurfgg04-gif, Discord-is-coming note, star-and-share ask; wired into GUARANTEES.md (the "under consideration" line became the live link), README Contributing, and the v0.1.0 release notes (PATCHED via the API, section inserted before "What's next")
- Created the 💎 Bounty label (the exact string bounty-hunter-style scanners query) + three scoped bounty issues (#5 soundness hunting, #6 adversarial corpus, #7 diagnostics wording) — honest amounts, no honeypot shapes, so the program reads trustworthy to the hunter-side tooling the owner flagged
- Discussion #1 bumped with the bounty announcement + three fresh asks (confusing diagnostics, what makes bounty tracks worth an evening, Discord vs Discussions)
- Batch V (row 13 follow-up): measured the blast radius first (all 10 examples gofmt-unclean, 670 diff lines), fixed four roots (emit_fn double indent, struct alignment, leading-import stray blank, inline loop labels), added the build gofmt pass for go/printer's spacing heuristics (cosmetic; failure falls back, go build stays the real gate), and caught a REAL CLI bug: bare-filename `hyb generate` died on read_dir("") — fixed + pinned by hyb/tests/generate_paths.rs
- Also fixed the 2 pre-existing clippy --all-targets findings (repeat_n, bool_assert)
- Full standard suites after every change: cargo 217/0 · clippy 0 (--all-targets) · go 5 pkgs vet/build/test · examples 10/10 · corpus 20/20 tripwire quiet · differential 9/9+11/11 · managed gate PASS · sample module green · blast radius 0 diff lines

Stage Summary:
- The bounty program is LIVE end to end: policy doc, label, three issues, announcement, release-notes section, radar-friendly shape
- The last documented follow-up in the plan is CLOSED; no open plan rows remain, standing work is community replies + bounty triage only
- One real user-facing bug found and fixed by the sweep (bare-filename generate), which is the process working: cosmetic work still ran the full suites

---
Task ID: 25 (Batch U/V close-out: fleets republished, first bounty PR triaged)
Agent: Super Z (main)
Task: Republish the release assets and Docker image from af86f26; triage the first bounty contribution; confirm no plan rows remain.

Work Log:
- CI green on af86f26 (push event); dispatched release-matrix (publish=true, v0.1.0) + docker-publish (v0.1.0) with required inputs after a 422 taught the input schema (sha+version[+publish])
- BOTH fleets SUCCESS from af86f26: all four tarballs + sha256 republished (11:23Z), GHCR :v0.1.0 + :latest refreshed
- First bounty PR arrived (tim777z, #8, closes #6): fetched the branch, ran the checker locally — the program does not parse (task-as-declaration + *y deref are not in the grammar; E0002 x2), so the harness would compare expected LOAN-ESCAPE vs actual E0002; the expressible loan-escape is already corpus 06; the SPEC's D2 marks closure-at-spawn as the open design point the PR was aiming at
- Approved both of the contributor's CI runs (action_required gate for first-time authors), posted the full triage on the PR (kind, exact, two paths forward: D2 design input on Discussions, or an expressible corpus edge) + a triage note on issue #6 (stays open); follow-up posted after their second commit deleted the file but left the dangling VERDICTS row
- Discussions swept: no new human replies yet (#1 at 3 comments incl. the bounty announcement)

Stage Summary:
- Release assets and Docker image now carry the Batch V fixes; the user-facing generate bug is out of the shipped artifacts
- The bounty program processed its first submission end to end within the hour: verify locally, approve CI, kind precise triage, paths forward — the process in BOUNTY.md working as written
- No open plan rows; standing work is community replies + bounty triage only

---
Task ID: 26 (the bounty pays for the first time: #10 + #11 merged, #8 triaged)
Agent: Super Z (main)
Task: Triage the three bounty PRs that arrived within hours of the announcement; merge the valid ones with credit; keep the discipline.

Work Log:
- PR #10 (ocakciomursel-pixel, issue #9): the E3004 registry entry documents `jobs <- move s.data` as a reject but the checker enforced only the top-level ident form — a GC field move or a move nested in a composite literal crossed a boundary unproven. Contributor reported AND fixed it (unwrap_parens + collect_move_operands, O5.4 move-form check, 2 regression tests). Verified in a worktree: cargo 216/0, corpus 20/20, differential FULL (one unexplained transient disagreement, then clean x4 + a fully captured clean run), examples 10/10
- PR #11 (muhammet424342, issue #7): the E3005 fix-it still taught the retired bare `managed { ... }` form; now names managed("reason", "category") with the closed category list (sendability note + diagnostics.json fixit + regression test). Verified: cargo 216/0, corpus, differential, examples, gate all green
- Both PRs' Actions runs approved through the action_required gate and green; merged #11 then #10 with credit in the merge messages
- Merged main re-verified: cargo 222/0 (+5 community tests), one merge-seam warning (unused import in freeze.rs) fixed, clippy 0, corpus 20/20, differential FULL, examples 10/10, gate PASS
- PR #8 (tim777z): triaged honestly — aims at the documented D2 open point (closure captures at task spawn) but the shape is not expressible in today's grammar and the file fails parsing; the expressible loan-escape is already corpus 06; no payout, two concrete paths forward posted; both CI runs approved so the contributor sees real results
- Hall of Fame updated (both landings), PLAN Decision Log entry written, STATUS updated, thank-you comments posted on issues #7 and #9

Stage Summary:
- The bounty program works end to end on real traffic: announcement to merged soundness fix in one session, with the verify-first discipline held throughout
- The checker is strictly stronger now: the documented-but-unenforced E3004 case is enforced, and no diagnostic teaches retired syntax
- Standing work remains exactly: community replies as they arrive + bounty triage

---
Task ID: session-27 (the stale family + the audit follow-ups + the conveyor)
Agent: Super Z (main)
Task: Owner directive — rebase the stale family (#28/#30/#32/#34) onto current main; land the composite-nil/flate2/pin-drift follow-ups; continue the open-PR conveyor.

Work Log:
- Fresh sandbox: Go 1.26.9 → /home/z/tools/go; Rust 1.99.0 via rustup minimal. Baselines on 8fb9238: cargo 290/0 · corpus 27/27 · differential FULL · examples 10/10 · check OK.
- Stale family, maintainer-assisted rebase (each PR verified hole-by-hole FIRST):
  - #28: 5/6 shapes already covered by pr-35's may-share; the live hole (composite literal embedding a PROJECTION, holder{m: h1.m}) ported through record_literal_ref's recursion; PR's parallel alias machinery documented as subsumed; tests landed with the A1b assertion adjusted to the seam's contract. Pushed to the fork (maintainerCanModify), CI approved, merged 73b3e49.
  - #30: all four holes live (closure pass never ran check_uses; select-arm bodies never collected). Ported as authored; tests unmodified. Merged e12adff.
  - #32: shadow-wipe/double-move/move+read live; cycle + shadowed-root already covered by may-share. Ported + extended symmetrically (share_scopes for may_share — main's freeze verdicts ride pr-35 edges). Merged d542690.
  - #34: all four laundering shapes live. Applied as authored (classify_move_ident + borrow-alias recorders). Merged 7063e33.
  - CI green on every rebased head before merge; merge seams verified (297/0 at the last); PRs now MERGED on GitHub; each author got a detailed seam-resolution comment.
- Follow-up (a) composite-nil (Batch X): check_composite_nils — struct-field/map-value/slice-elem slot-typed nil rejection (recursive, managed-transparent), wired at define/rebind/return/check_site; corpus 28 + 6-shape unit test; parse pin 27→28. Merged 6c6b66a.
- Follow-up (b) flate2 early-fail (Batch Y): UNBOUND_RUST_CRATES + EmitError::unbound_crate — both shapes (bare call via emit_call's gate, explicit import) refuse at emit with the honest message; check stays type-pure (corpus 14's A7 fixture intact); +2 emit tests; stray a.out.go swept + gitignored. Merged fa45fda/bb8dba6.
- Follow-up (c) pin-drift guard (Batch Z): hyb/build.rs — rerun-if-env-changed (stale-bake fix) + compile-time refusal when the building rustc != the baked pin, with the three honest ways out; fails open on unparseable rustc; check-versions syncs DEFAULT_PIN. All three behaviors verified (pass/refuse/explicit-match). Merged 84a5e3c.
- Conveyor: #41/#56/#58/#59 closed as landed-with-credit (heads were rebased copies; landing SHAs posted). Dup pairs: #80+#83 → ported #83's message+corpus with #80's root-group union (issue #75 repro now rejects "may-share peer of parameter"); #81+#82 identical commits → exit-code contract in both harnesses; #78+#86 → #86's E3007 registry design won, #78 credited. pr-24 rebased onto today's seam and merged (loop back-edge, fixes #23). pr-74 ported (Call arm in record_may_share — struct-laundering + generic-identity forms; corpus renumbered 31). #74/#80/#81/#82/#83/#86/#78 closed with credit.
- CI incident + root cause: Batch X's corpus row declared "REJECT N1" before E4001's registry entry carried tier0_rule_id — 6 CI runs red from 6c6b66a; pr-24's rebase added the registry line and CI recovered (fed3ebd → green through 91c6172). Local lesson recorded: my `| tail` gates masked the harness exit code; re-verified all gates with pipefail (exit 0 for real).

Stage Summary:
- corpus 31/31 · cargo 322/0 · clippy 0 · differential FULL (21 reject-side) · examples 10/10 · main CI green at every push since fed3ebd.
- PLAN follow-up row marked LANDED with the decision-log entry; STATUS rewritten.
- Remaining queue (14, next session): #8 #22 #63 #64 #65 #67 #68 #70 #71 #76 #84 #85 #88 #90 — same discipline: repro-verify on main first (several may already be fixed by today's landings), port-or-close with credit, gates WITHOUT piped tails.

---
Task ID: session-28
Agent: Super Z (main)
Task: Check all pending PRs; add the $500 Wildcard Track to the bounty program (bounty page, CONTRIBUTING, repo docs, moonshot.md); AI-managed intake/review/milestones/payouts; active solicitation.

Work Log:
- Fresh sandbox: re-pinned Go 1.26.9 + Rust 1.99.0; baselines re-established.
- Discovered local main was ~100 commits stale; rebased onto origin/main (f99d6d0) — the repo already had BOUNTY.md ($500 ladder), CONTRIBUTING, CONTRIBUTOR_RECOGNITION, SECURITY; re-integrated the wildcard work INTO BOUNTY.md rather than a parallel BOUNTIES.md.
- Wildcard Track landed (commit 11742cc): BOUNTY.md Track 3 (another $500/cycle; $250 M1 acceptance + $250 M2 published learning, success or failure alike; no prototype required; breaking changes/rewrites/"impossible" ideas allowed; failure-with-knowledge paid), MOONSHOT.md (ledger + learnings log + failure wall + monthly review window + solicitation calendar), wildcard.yml intake, config.yml contact, CONTRIBUTING/README/GUARANTEES/PLAN cross-links, wildcard+moonshot labels, announcement posted as Discussion #165 with five solicitation prompts.
- PR marathon: 54 open PRs found (recon: pr_recon.json); 44 fork-PR workflow runs batch-approved via API.
- Round A (tools/CI, 14 merged): #63 #64 #85(seam) #71 #122(fixture seam-fix) #127 #116 #133 #102 #135 #65 #129 #154 #76(darwin bridge + Mach-O detection). Battery: cargo 326/0, python 56, corpus 31/31, differential FULL, examples 10/10, gate PASS. (One false alarm diagnosed: a stale Oct-14 compiler/target/debug/hyb produced fake differential disagreements; fresh shared-target binary = FULL.)
- Round B (parser/emit, 8 merged): #107 #108(seam: `case` reserved + pr-22 contextual default) #136 #145 #150(spec primitives + const folding) #90 #101(seam fixes: JSON boundary, dup arm line, truncated test) #22(corpus renumbered 32-35, counts 35). Workspace 361/0, corpus 35/35, differential FULL, go suite 6 pkgs. Pushed 96f0bc4.
- Round C (checker stack): #110→#117→#126 stacked, landed with seam repairs (Call-arm braces, corpus renumber 36/37/38, parse_all rebuilt); #142(corpus 39) #151; SEMANTIC CONFLICT: pr-110's two accept-side precision tests predate fef40da — verified against f99d6d0's checker that the shape was already rejected on main; amended tests with logged decision, PLAN follow-up (N1-p). #162 (generalized collector; major seam: took pr-162's record_may_share wholesale + re-added the #110 FnLit arm; corpus 40/41) #140(self-shadow origin) #128(const-move; test attribution amended E3001→E3003/E3006, logged) #156(corpus 42) #157(E6001) #163(E6002; JSON seam fix) #68 #70. Pushed through b0617ea.
- Triaged ALL remaining open PRs in writing (#132 #121 #159 #153 #106 #99 #67 #84 #88 #93 #115 #91 #96 #113 #97 #8) with queue order and seam notes; #161 closed as duplicate of #150 with credit; #8 honestly triaged (in-grammar rewrite invited; rewritten intent ACCEPTS soundly).
- Close-out: STATUS.md rewritten, PLAN.md conveyor + decision rows, this worklog.

Stage Summary:
- 20 PRs merged this session; 1 closed; 16 queued with written triage. Corpus 31→42, cargo 322→398, differential 21→29 rejects, all suites green at every boundary.
- Wildcard Track fully operational: rules, ledger, intake template, labels, announcement, solicitation prompts.
- Key artifacts: BOUNTY.md Track 3, MOONSHOT.md, wildcard.yml, Discussion #165, seven corpus renumberings with maintainer-integration notes, four logged semantic-conflict decisions.
- Next session: the triaged 16-PR queue (#91+#96 pair next), first community wildcard entries, release republish.

---
Task ID: session-10
Agent: Super Z (main)
Task: Continue PR conveyor + wildcard announcement/ledger wording review; keep CI honest.

Work Log:
- Fresh sandbox again: Go 1.26.9 + Rust 1.99.0 reinstalled (background nohup died silently; synchronous dl.google.com worked); symlinks /home/z/.rustup + /home/z/.cargo → /home/z/tools/*. Baseline cargo green.
- Found + fixed the CI-down P0: since 96f0bc4 (pr-22 maintainer integration), ci.yml had an unindented DUPLICATE py_compile line (every Actions run died 0-jobs) and Makefile lost a recipe TAB. Merged pr-172 (deletion — the line was a duplicate), maintainer-integrated pr-199's Makefile hunk (8b9e1d6 + cb53ead). Push-run GREEN. #172/#199 closed with credit. NOTE: merged-on-local-evidence documented in the merge message (CI was the thing being repaired).
- Keyword auto-close repair: #67 REOPENED (closed by `fixes #67-adjacent` in pr-68's merge message), #121 acknowledged (closed by `fixes #121` typo in pr-122's merge; author had opened successor #197). Conveyor rule recorded: closing keywords are for ISSUES only.
- BOUNTY.md wording fix: "What qualifies" said "Two tracks, ranked together" while listing the wildcard as a third separate track → "Two tracks ranked together, plus a third that stands apart". Verified wildcard wording consistent across BOUNTY/MOONSHOT/CONTRIBUTING/README/Discussion #165 ($500, $250+$250 M1/M2, 48h triage, monthly window, 5 solicitation prompts all agree). No wildcard proposals yet.
- Wave recon (42 new PRs #164-#229 + 15 old): triage comments with family assignments + queue order posted on ALL open PRs; 30 action_required fork runs approved; zombies #76 (zero unique commits) + #162 (integrated as 110bcf0, corpus byte-identical) closed; #124 corrected (script half landed, test_package_release.py never did — kept open, tests wanted).
- Landings (each with local verification + suite boundary):
  - Batch A tools: #181 (payload repr-quoting), #209 (shape --check read-only), #211 (EXPECTED-VERDICT fail-closed), #168 (hazard exit-contract: exit 2/SIGABRT), #221 (managed-report envelope fail-closed), #177 (contradictory JSON counts) — STATUS/PLAN conflict chunks resolved both-sides.
  - Copy family: #180 merged (record_copy_may_share + shape.rs element resolution + corpus 43/44; fixes #178); #179/#182 closed as same-fix with credit. Racing repro rejects E3003, []int control accepts.
  - Corpus: #176 maintainer-integrated as #45 (slots 32-44 were taken; payload/verdict unchanged, verified REJECT A1c/E3001; VERDICTS row + distribution 45 programs).
  - #171 merged (channel element types E3011, fixes #146) + seam fix: E3011 registry entry landed before E3010 → swapped (hy-diag ascending invariant); parse_all count bumped 44→45 for corpus 45.
  - #170 merged (short-circuit + eval order + if-init for expression `?`, fixes #158; pre-merge main emitted the RHS of `false && touched()?` UNCONDITIONALLY); #169 closed as same-fix with credit. Resolved #170's ci/Makefile hunks to HEAD.
- Shared-target poisoning incident: after building PR worktrees into the shared CARGO_TARGET_DIR, 6 corpus cases + many checker tests "failed" (accepted everything). Fresh-target rebuild: ALL GREEN (442/0, corpus 45/45, differential FULL, examples 10/10). RULE: PR-branch builds use their own CARGO_TARGET_DIR (/tmp/prtarget-N); shared target only for the main tree.
- Known issue: ci.yml push filter still `branches: ain]` (mangled '[main]'). Multiple fix attempts (python literal, sed, atomic script, Contents API) all lost to what looks like non-ASCII lookalike gremlins in tool input round-trips; CI empirically FAILS OPEN and runs on main pushes (proven all session). Fix scripts staged at /home/z/my-project/scripts/fix_branch_filter.py + fix_filter_atomic.py + fix_filter_via_api.py — next session: fetch bytes via API, replace by line-index, PUT.

Stage Summary:
- 14 PRs processed (11 merged + 3 dup-credited) + 2 zombies closed + 1 reopened + ~40 triaged. corpus 45, cargo 442/0, differential FULL, examples 10/10, tools python 104/OK.
- Pushed: cb53ead → aedfa2d (Batch A) → 57ab16c (#180) → 93aea3d (corpus 45) → 8d05fd7/8389056 (#171 seam) → 194715b (#170) → bfefd10 (BOUNTY wording). CI green on every push.
- Next session queue: slot-loan stack #214→#218 + #222; const-composite #190/#192; match #173/#210; fail-closed trio #185/#224/#227; closure family #207/#213/#220/#226/#229; defer pair #223/#99; #197; #191; #198; #106 design note; old queue rebase assists; ci.yml filter via API.

---

## Session 12 (2026-10-17) — CI recovery, wildcard first harvest, A-tier wave landings

Started at 00610f7 (+2a147fe/#359 mid-flight), ended at 40f5315. Toolchains re-pinned after sandbox reset (Go 1.26.9, Rust 1.99.0 via the build.rs pin).

- **CI P0**: main's ci.yml carried committed merge-conflict markers (issue #358) — every run failed at YAML parse since ~22:32Z 10-16. Merged #359 (Ratanlavate; its head had the window's only green run), closed #337/#339 same-fix with credit. Push runs verified green (4f5c003, 0c8b537, 6e05c68). The earlier "zero-jobs quota" suspicion was wrong — jobs always ran; the workflow file itself was unparseable.
- **Wildcard window (user's first ask)**: 19 `[wildcard]` issues had arrived; none were triaged. All 19 got written 48h-verdict comments + labels; **17 entered the MOONSHOT ledger as W-001…W-017**, 2 duplicate-credits (#175→W-001 family, #308→W-005). Families: capabilities-replace-the-alias-graph (W-001..004, W-014, W-017), message isolation (W-005), cancellation/protocol liveness (W-006..010), toolchain rewrites (W-011..013), targeted capabilities (W-015/016). Review window scheduled 2026-11-01 with slots in **discussion #365**; spike PR #336 attached to W-001. MOONSHOT ledger + review-windows table committed.
- **Wave-2 triage**: written verdicts on every open PR #248–#364 (99 comments; 7 closes). A-tier all landed:
  #357 (orphan else, fixes #356; end-to-end go run verified) → #360 (make 3-arg, fixes #353; corpus 51) → #355 (const/container guards, fixes #354; two maintainer repros) → #343 (predeclared shadow, fixes #326; corpus renumber 51→52 conflict resolved; #329 same-fix credit) → #323 (inclusive range wrap, fixes #322; cherry-picked off a divergent 17k-line fork history and re-united with #218's loan-frame machinery) → #266 (E2002, fixes #265; lib.rs union kept both guards) → #274 (E3104, fixes #273; via_addr port).
- **Verified no-delta**: #362 closed after 11/11 of its own tests passed on main + 8/8 probe parity + corpus parity. Issue #361 remains OPEN with 4 pinned wrapped-move shapes (field/index/paren-slice/call wraps of locals) — the real remaining P0 work.
- **Self-caught incidents**: #343 integration dropped #355's make-shape MapLit resolution (caught by const_write battery after push; restored in 49f5361 with the lesson recorded); accidental deletion of #213's test file (same name as #362's) caught before commit and restored.
- **Suites at close**: cargo 0 failed (37 binaries), corpus 52/52, differential FULL 15+37, main CI green.
- **Next session**: landing queue per STATUS.md — #306, #230, #251, #244, #240/#245, #247, #296, #311/#302, #234, const family, emit-lowering batch; then tools/kaggle batch; wildcard M1 verdicts 2026-11-01 in #365.
