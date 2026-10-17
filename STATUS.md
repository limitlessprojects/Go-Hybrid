# STATUS (for the human — 15 lines max)

- Phase: **MVP DONE + v0.1.0 released + community OPEN** — session-17: **7 more PRs landed (#420 #447 #421 #290 #236 #208 #235) + 8 same-fix credit closes** — cargo 808/0, corpus 60/60, differential FULL (17+43)
- **Operand-type family closed**: #420 (receive E2008/E3005, fixes #409) + #447 (for-in E2009/E3005 superset of #424 — fail-closed unknowns, method resolution, generic-in fold; fixes #411); credit closes #413/#414/#424/#426/#427
- **Defer liveness closed**: #421 (exantiv, fixes #415+#438) value-precise deferred-binding freeze — agent XOR analysis REFUTED the old "#418 covers" plan (#418 missed a5/c4/c5, #422 missed b1/b1r); #418/#422 tests spliced as pins (3 wording pins → verdict pins, logged)
- **Const family closed**: #290 (neg-zero E6001) → #236 (undefined names E1001, fixes #228-A) → #208 (non-const const-init E6003, fixes #205; #433 credit-closed) → #235 (select-send move marker C5; 3 Send-destructure sites gained `..`)
- Next queue (recipes READY in worklog session-17/D): const-overflow stack **#291 → #297 → #300 → #232** (#232 needs 5-line ConstInt adaptation after #297; #300 leaves struct-field-store gap = follow-up issue), then **#242 as scenario-port** (11/15 live failures = goalposts), then **#288** (defer `?` hoist, validated)
- Tools batch after that (order re-derived): #364 → #333(cargo recount at merge) → #387 → #383 → #316 → #388 → #403 → #325 → #299(+ci line) → #423
- Wildcard: 11 ledger-attach comments posted; MOONSHOT rows W-006/7/9/14/15/16/17 + **new W-018** (untalsix #378, spike #384) + windows table updated; W-015 row gap fixed (#379 now named); #416 closed not-ready (silent descriptors.md edit), #464 on governance hold; **M1 verdicts 2026-11-01 in #365**
- Outreach targets (no artifact): W-002/003/004/006/008/010/011/013/016; CI quota HEALTHY (zero zero-job failures across last 50 runs)
