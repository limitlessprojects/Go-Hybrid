# STATUS (for the human — 15 lines max)

- Phase: **MVP DONE + v0.1.0 released + community OPEN** — session-14: queue pull resumed (**#230 landed**) + **second wildcard spike (W-005, PR #412) race-verified**
- **#230 landed** (lienhoatienphung-svg, fixes #229 P0): nested closures now check against their ENCLOSING closure's env, not the top-level FnState — outer-closure maps/moves no longer launder past E3001/E3003/E3004; merge resolution grafted main's #306 if-init eval order + #374 non_owned_origin materialization into the PR's restructured walk; race demo: pre-merge main's emitted Go races under -race, post-merge the repros reject
- **W-005 spike (PR #412, spike/w005-message-isolation)**: no-compiler artifact per the issue's own first-milestone directive — memoized deep clone keyed on the REFERENCED object (cycles/internal aliases/pointer-keys survive inside the copy; chan identity preserved; FAIL-CLOSED on funcs/uintptr/unexported fields); 7/7 pins green under go test -race; demo r1 = the #229 repro under isolation, zero race reports
- **W-005 M1 verdict: CONDITIONAL** — correctness proven; 2× spawn budget fails at ~40,000× (10k-node graph ≈ 100ms vs 2.5µs spawn); narrowings: opt-in isolation / bounded actor mailboxes / COW; published on #342 + RESULTS.md
- Suites at close: cargo workspace **594/0** (591 + #230's 3 pins) · corpus **52/52** · differential **FULL (15+37)** · main CI green on the #230 merge (e63415a)
- Wildcard M1 verdicts due **2026-11-01** in discussion #365 — two running artifacts now: W-015 (PR #379) + W-005 (PR #412)
- Wave-2 landing queue continues: **#251 (named results)** leads, then #244, #240/#245, #247, #296, #311/#302, #234, const family, emit-lowering batch; new arrivals #366–#377 (tools/fuzz batch), #373, #364; 9 PRs pending CI approval in the queue's per-landing flow
- Standing work: old-queue rebase assists (#67/#88/#93/#106/#197/#198/#99/#223), tools/kaggle batch, CI registration checks (#325/#330/#333)
- Last review gate: G3 PASSED 2026-10-13; community review is a standing input to the queue
- Local tooling fix: UTF-8 checker-output decoding + portable CLI fixtures; 25 focused Python tests pass on Windows, four encoding cases fail on base. Full compiler suites and upstream CI remain unverified.
