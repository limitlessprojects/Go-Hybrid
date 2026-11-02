# 💰 The Hybrid Bounty

We pay for **real soundness bugs**, **useful improvements**, and
**revolutionary ideas** in Hybrid. The main pool is **$500** per cycle,
split across the **top 10 most meaningful contributions** — and the
**Wildcard Track adds another $500** per cycle for the kind of idea a
normal feature bounty would reject. Open to humans and AI: we judge the
report, not the author.

The maintainer keeps the final say on whether a report qualifies, its tier,
and its payout. Every decision gets a written reason in the issue.

## The ladder

| Place | Payout |
|-------|--------|
| 1st | **$75** |
| 2nd | **$60** |
| 3rd | **$50** |
| 4th to 10th | **$45 each** |

Total: $500. The top three take a larger share so the ladder stays
meaningful; exact amounts may be tuned before a cycle closes. Winners are
announced in the release notes and listed in the Hall of Fame below.

## What qualifies

Two tracks ranked together, plus a third that stands apart:

1. **Soundness bugs (the main track).** A program Hybrid **accepts** but
   that is **unsound**: a data race on shared mutable state crossing a task
   boundary, a managed block escaping its contract, a stack-budget lie in a
   bridge call, a sendability proof granted without grounds. The bar is
   [GUARANTEES.md](GUARANTEES.md): an accepted-but-unsound program is a P0
   by definition.
2. **Useful improvements.** Fixes, adversarial corpus tests, diagnostic
   wording, docs, tooling. Meaningful beats large; a small fix with a test
   outranks a big idea with no repro.
3. **Wildcard proposals — a separate track with its own $500.**
   Revolutionary, contrarian, high-risk ideas that the ladder would
   otherwise filter out. No prototype required; breaking changes,
   new paradigms, rewrites, and "impossible" ideas explicitly allowed;
   **failure is a valid outcome if it produces knowledge, and every
   outcome — including failures — is published.** Full rules, ledger, and
   the learnings log: [MOONSHOT.md](MOONSHOT.md); quick version below.

## Severity tiers

| Tier | Meaning | Example |
|------|---------|---------|
| **Critical** | Silent data race, memory unsafety, or wrong results in accepted code | shared mutable cell read from two tasks with no proof |
| **High** | Soundness hole with narrower reach | an escape hatch skips a boundary check |
| **Medium** | A guarantee weaker than documented, or a misleading pass | sendability accepted on an unproven path |
| **Low** | Rough edges: diagnostics, docs, tooling gaps | confusing error text, missing repro hint |

Tier shapes ranking: a reproduced Critical outranks a nice-to-have.
You may propose a tier; the maintainer assigns it. Scoring exists to
prioritize fixes, not to be gamed. How the top programs run this is
documented in [research/bounty-study.md](research/bounty-study.md).

## What does NOT qualify

- Crashes that produce a **clean diagnostic** (HYB-E0002 and friends).
  The fail-closed system doing its job is not a bug. If the diagnostic
  text is wrong or missing a fix-it, that is the Low track.
- Findings in **dependencies we do not own** (report them upstream, then
  tell us what you found; if our usage amplifies them, that part is ours).
- Style preferences, benchmarks without a correctness claim, and reports
  about programs the checker correctly REJECTS.
- Anything already on the known-issues list or already fixed on main.

## What makes a report survive triage

1. Check [GUARANTEES.md](GUARANTEES.md) and the issue tracker first.
2. Run against **latest main**, not a release tag.
3. Bring the smallest `.hy` file that shows it, the exact commit, and
   what you expected vs what happened.
Reports shaped like this get reproduced fast, and reproduction is the
only door to a payout.

## Payment rules, plain and short

- **Pay only after reproduction.** A soundness report must show a real
  accepted-but-unsound regression against our corpus: the compiler accepts
  it today, with steps, the exact commit, and a minimal `.hy` repro.
- **No payment for theories, duplicates, already-known issues, or anything
  we cannot reproduce.** The known-issues list lives in
  [GUARANTEES.md](GUARANTEES.md) and the issue tracker.
- **CI must pass on our side.** A fix PR has to go green on our GitHub
  Actions run at least once before it gets the final review that decides a
  merge. We never merge on local-only evidence.
- **Duplicates:** the first reporter takes the payout; later duplicates get
  Hall of Fame credit and a link to the original.
- One contribution takes one ladder slot. Ranking is by meaning, not size.

## How to submit

1. **Bugs:** open an issue with the
   [`soundness` template](https://github.com/limitlessprojects/Go-Hybrid/issues/new?template=soundness.yml).
   **Improvements:** a normal issue or a PR. **Wildcard proposals:** the
   [`wildcard` template](https://github.com/limitlessprojects/Go-Hybrid/issues/new?template=wildcard.yml).
2. Include: what happens, what should happen, the exact commit, and the
   smallest repro (a `.hy` file or a test) — wildcards instead name the
   sacred cow they kill and how we would falsify the idea.
3. We aim to **acknowledge within 48 hours** with a first verdict:
   accepted, needs info, or duplicate, with reasons.
4. Fix PRs must pass CI on our side, then get final review.
5. Winners are announced when a cycle closes, in the release notes and here.

## Track 3 — the Wildcard Track, quick version

**Purpose: incentivize revolutionary, contrarian, high-risk ideas that a
normal feature bounty would reject.** We are hunting for breakthroughs,
not incremental improvements — the unique approaches or additions that
could make a paradigm shift without breaking everything, or that are worth
it even when they do (the cost-to-ROI call is ours; your job is honesty
about the cost).

- **Budget: another $500 per cycle**, separate from the ladder. One
  wildcard winner per cycle: **$250 when the proposal is accepted into the
  ledger, $250 when its learning is published — success or failure alike.**
  A fast, honest failure collects its second half; publishing the failure
  *is* the deliverable.
- **No working prototype required.** This is purely a place for us to
  analyze people's crazy ideas and thoughts. A well-argued paragraph is a
  complete submission.
- **Breaking changes, new paradigms, rewrites, and "impossible" ideas are
  explicitly allowed.** Say which sacred cow dies and why — SPEC.md,
  GUARANTEES.md, and the `.hy`/`.go` split are all fair game.
- **Everything is published:** the ledger, the review schedule, the
  milestones, the payouts, and the learnings — including the failure wall —
  live in [MOONSHOT.md](MOONSHOT.md).
- **We actively solicit, not just wait:** each cycle names the assumptions
  we most want attacked (see the solicitation calendar in MOONSHOT.md).
- **Ops run by the project's AI maintainer:** proposal intake (triage
  within 48 hours, same as bugs), review scheduling (a public monthly
  review window), milestone tracking (the MOONSHOT ledger is the single
  source of truth), and payout preparation — with the maintainer keeping
  the final say and every decision getting a written reason, same as the
  ladder.

## Scope

**In scope:** the compiler crates (`compiler/crates/*`), the runtime Go
packages (`runtime/go/*`), the `hyb` CLI, the corpus, and the CI workflows
in this repo.

**Out of scope:** anything outside this repo, denial of service, bulk
automated scanning without a written report, social engineering, and
attacks on users or infrastructure.

## Safe harbor

Good-faith security research that follows this document is authorized, and
we will not pursue legal action over it. Stay in scope, use test accounts
and your own repos, and stop and report the moment you find something. When
in doubt, ask in
[Discussions](https://github.com/limitlessprojects/Go-Hybrid/discussions)
first.

## AI-assisted reports

AI-assisted hunting is welcome, with honest rules that keep the queue alive:

- Reports are judged on **verified impact**, never volume.
- Credit goes to the **GitHub account** that submits, whether a human or an
  AI operator stands behind it.
- Minimal repros only. No bulk dumps, no AI-generated fluff, no repeated
  near-duplicates.
- Flooding or spam disqualifies the account from the current cycle.
- If invalid volume ever floods the queue, the program **pauses** with a
  note in Discussions rather than dying quietly or banning in bulk. We
  would rather stop than stop being fair.

## Recognition beyond cash

The full role ladder (Bug Hunter, Security Guardian, Builder, Core
Contributor) and how to earn each is
[CONTRIBUTOR_RECOGNITION.md](CONTRIBUTOR_RECOGNITION.md). In short:

- **Hall of Fame** (below) for every valid report and merged improvement.
- **Release notes shoutouts** for every winner.
- **All-contributors credit** on request: comment
  `@all-contributors please add @username for security` (or `bug`, `doc`,
  `code`) on your issue or PR and merge the bot's README update.
- Reliable reporters get invited to help triage, then earn wider repo roles.
  The ladder is: report, then triage help, then regular contributor, then
  trusted roles with ownership.

## Hall of Fame

| Who | What | When |
|-----|------|------|
| [zuluagh](https://github.com/zuluagh) | **Two registered-never-raised codes brought live**: PR #266 (issue #265) — E2002 fires when a multi-value call is bound as one value (`p := minmax(3, 9)` passed hyb check and failed go build); PR #274 (issue #273) — E3104 fires for address-of taken on a map-slot loan (`p := &n` on a loan lowered to a copy then took a general pointer; T24's spec claim now enforced) | 2026-10 |
| [salvocanci](https://github.com/salvocanci) | **Inclusive-range terminator** (PR #323, issue #322): `for v in lo..=hi` at the type maximum wrapped (int8 `127..=127` never stopped); the loop now drives a continue-safe stop-flag evaluated first in the body — the body runs once at `hi` and exits | 2026-10 |
| [rickymulya](https://github.com/rickymulya) | **Predeclared-shadow resolution** (PR #343, issue #326): a struct named after a predeclared type (`struct rune { m map[..] }`) was classified as the primitive, silently skipping aliasing checks — named_shape resolves user structs and aliases first, with depth-guarded type resolution; same-fix credit to ajithnair1808-max's independent #329 | 2026-10 |
| [sonson0910](https://github.com/sonson0910) | **Class-4 const + container guards** (PR #355, issue #354): const closure-containers can no longer cross task/chan boundaries under the pure-value exemption; `make(map[K]fn(){})` no longer erases to Map(Unknown, Unknown) for the closure-container move guard; type-aliased closure fields resolve through type_aliases; interprocedural const-mutation checks reach closure/method bodies | 2026-10 |
| [Ravs0](https://github.com/Ravs0) | **Diagnostic accuracy** (PR #360, issue #353): `make([]T, len, cap)` — valid Go — was rejected with a builtin-arity diagnostic claiming Go rejects it; the 3-arg slice form is accepted end-to-end (corpus 51) | 2026-10 |
| [Ratanlavate](https://github.com/Ratanlavate) | **CI recovery + match emission fix**: PR #359 (issue #358) — dropped the committed merge-conflict markers that made every workflow run fail at YAML parse (first green run of the window was this PR's own head); PR #357 (issue #356) — an unguarded first `some`-arm no longer chains an orphan `else`, so accepted matches emit buildable Go again (the accepted-implies-buildable contract) | 2026-10 |
| [area523ufo](https://github.com/area523ufo) | **Six landings in one conveyor run — the slot-loan/loop-backedge stack**: PR #214 (issue #194) — a call-initialized loop local erased the backedge freeze on its alias root (`m := same(agg)` in a loop moved one map into two tasks); PR #215 (issue #195) — scope_pop sweeps may_share peers, not aliases only; PR #216 (issue #196) — loop-backedge analysis runs inside closure bodies; PR #217 (issue #202) — slot-loan frames scoped to closure bodies (store-backs no longer hoisted out of fns); PR #218 (issue #203) — store-backs flush before return/break/continue; PR #173 (issue #143) — match exhaustiveness (E5001) enforced: non-exhaustive, none-only, and _-first matches reject with the missing variant names. Plus credited same-fix takes on #222/#227 | 2026-10 |
| [milkingndb-oss](https://github.com/milkingndb-oss) | **The five-family fail-closed swing** (PR #224, issue #201): incomparable ==/!= on map/slice/func/incomparable structs, var-ascription constant overflow/truncation + negative shifts, redeclarations, invalid receivers, invalid array lengths — 16 repros from the issue's own tables now reject fail-closed with dedicated codes E1003/E2006/E2007/E6004/E6005; 33-test battery | 2026-10 |
| [CarlitoDon](https://github.com/CarlitoDon) | **Unbuildable declarations** (PR #185, issue #134): unused `:=` bindings (incl. `_ := 5`), unused imports, and repeated names on one `:=` now reject fail-closed as HYB-E1002 (rule BUILDABLE); corpus 45→49 with the four pre-registered fixtures | 2026-10 |
| [rickymulya](https://github.com/rickymulya) | **Match multi-arm emission** (PR #210, issue #204): emit_match kept only the LAST some arm — guarded arms were silently deleted; all some-arms now lower (guards chain, all-guarded shapes fall through a hyMatched flag), with a credited same-fix take on #203 (pr-222) | 2026-10 |
| [sonson0910](https://github.com/sonson0910) | **Closure origin/may_share preservation** (PR #207, issue #206): type-ascribed locals, assignment rebinds, self-rebinds, and parameter peer rebinds inside closures preserve non_owned_origin and may_share edges — captured-state moves across task boundaries no longer slip through; PR #213 (issue #212) — boundary handles join may_share/A2 loan conflicts, same-stmt share moves freeze, and closure const scanning rebuilt on a single ordered pass | 2026-10 |
| [salvocanci](https://github.com/salvocanci) | **Defer/builtin hardening** (PR #191, issue #187): defer non-call rejects E2001; make/len/cap/delete/close/new literal and arity misuse rejects E2005; also the channel element-type landing (PR #171, issue #146) — sending into a mismatched-element channel rejects E3011 | 2026-10 |
| [Ratanlavate](https://github.com/Ratanlavate) | **Three more landings this week**: PR #47 — the installer launcher keeps a caller-provided `HYB_RUNTIME_DIR` (issue #20) and the row-18 `shape.py --check` CI step got the absolute path it needed (main's ci.yml had been red since the step started cwd-ing elsewhere); PR #42 (issue #38) — a call that returns a reference-typed value is recorded as an alias of its argument (`same(m)` freezes with `m` on move); PR #35 (issue #27) — may-share tracking: shallow struct/array copies, re-slices, projections, and composite literals embedding a live binding all freeze with their source on `move` | 2026-10 |
| [ocakciomursel-pixel](https://github.com/ocakciomursel-pixel) | **Two more soundness landings**: PR #52 (issue #51) — `atomic.pointer[T]`/`atomic.value` left the sendable class 2 (the atomic makes the pointer swap race-free, NOT the pointee — non-scalar atomics now fail closed E3005 citing GUARANTEES row 11); PR #61 (issue #60) — the non-owned-move guard: function/closure parameters, method receivers, closure captures, and non-channel range variables carry a `non_owned_origin`, and `move` of them at a boundary rejects E3004 | 2026-10 |
| [area523ufo](https://github.com/area523ufo) | **Three soundness landings**: PR #59 (issue #57) — only top-level fns enter the fn table (a method can no longer impersonate a same-named top-level fn) plus pointer-receiver shape preservation; PR #56 (issue #54) — parallel A3 map-slot loans, A6 field-borrow checks for parallel/multi-return stores, view-loan window extension; PR #41 (issue #40) — scope events preserve MULTIPLE events per index (a nested construct and its enclosing if/for ending together used to drop a Pop and leak bindings) and select-recv defines get their own scope | 2026-10 |
| [trikiman](https://github.com/trikiman) | Multi-value soundness landing (PR #49, issue #48): multi-value unpacks resolve per-position shapes/types from the declared fn results instead of stamping the first result's shape on every target; parallel assigns evaluate RHS in the pre-assignment environment and record reference aliases | 2026-10 |
| [zidirahim617-del](https://github.com/zidirahim617-del) | Const deep-immutability landing (PR #44, issue #37): writes through a `const` root (rebind, element write, field store) reject HYB-E3006 — a file-level const lowers to package-level state every task body reaches, so an accepted write was a data race with zero spawn-site evidence. Plus two doc landings (PRs #46, #26): GUARANTEES rows 1/3 now link corpus files that exist, and the README shows the real `managed("reason", "category")` hatch | 2026-10 |
| [daniellachev](https://github.com/daniellachev) | Tooling landing (PR #45): `hyb generate` excludes `*_test.go` from production package inference — a sibling test package must not choose the generated package name; three regression tests | 2026-10 |
| [xiaokaizhou](https://github.com/xiaokaizhou) | Harness honesty (PR #36): the differential loop no longer counts an unrelated runtime panic as the documented RUNTIME-HAZARD — the run's first diagnostic line must match the pre-registered hazard exactly | 2026-10 |
| [BountyScout208](https://github.com/BountyScout208) | Harness honesty (PR #58, issue #55): the corpus harness fails (exit 2) when zero EXPECTED-VERDICT cases register — a mispointed `--corpus` used to read as a full pass | 2026-10 |
| [willmanntatel-alt](https://github.com/willmanntatel-alt) | Gate hardening (PR #62): the managed gate fails closed when managed blocks cannot be paired to unambiguous source lines — the old `?` fallback let a later block overwrite an earlier one | 2026-10 |
| [velardream-ctrl](https://github.com/velardream-ctrl) | Gate soundness report (issue #53): `managed_gate.py` ignored `hyb check`'s exit status — a rejected program read as zero blocks, reported PASS, and `--rebaseline` could erase a reviewed baseline; the gate now fails closed on a nonzero checker exit | 2026-10 |
| [Ratanlavate](https://github.com/Ratanlavate) | **Two soundness landings in one day**: PR #17 typed `new(T)` as a pointer, not a primitive — `task w(new(S))` used to share a `*S` across tasks with zero diagnostics (issue #16); PR #13 stopped `let`-bound mutable GC values from taking the class-4 immutability shortcut at spawn boundaries (issue #12). Both with regression tests | 2026-10 |
| [muhammet424342](https://github.com/muhammet424342) | Wording landing (PR #11, issue #7): the HYB-E3005 fix-it taught the retired bare `managed { ... }` form; it now names the real `managed("reason", "category")` hatch. Corpus landing (PR #15, issue #6): a hand-derived A1c pair — a live map-slot loan crossing a spawn is REJECT, its copy twin ACCEPT | 2026-10 |
| [LubuSeb](https://github.com/LubuSeb) | **The shadowing P0** (issue #14): a minimal repro with a negative control showing an inner-block `m := 1` laundered an outer shared map past a spawn — zero diagnostics, two write/write data races under `go -race`. Reproduced before the ack; the checker's env is now scope-aware (corpus 24) | 2026-10 |
| [ocakciomursel-pixel](https://github.com/ocakciomursel-pixel) | **First soundness landing** (PR #10, issue #9): reported and fixed the unenforced E3004 case — `move s.data` and nested `move` forms at spawn/send boundaries slipped through unproven; two regression tests pin the registry's own example | 2026-10 |
| [ssmurfgg04-gif](https://github.com/ssmurfgg04-gif) | First community review: four substantive asks that shaped [GUARANTEES.md](GUARANTEES.md), the one-command install, and the `--deny-managed` hatch | 2026-10 |

Valid bug reports and merged improvements land here. Be next.

## A kind ask

If Hybrid interests you, **starring and sharing the repo helps more than
you think**. Bug reports make a project better, but stars and shares decide
whether anyone is around to use the fixes. If everyone reports faults and
nobody signals the project exists, the fixes die quiet. It costs nothing,
and it brings the next contributor.

## Community

We are **working on a Discord**; it will be linked here the day it opens.
Until then [Discussions](https://github.com/limitlessprojects/Go-Hybrid/discussions)
is the town hall: questions, ideas, show-and-tell, and bounty talk all land
there, and we answer every thread.

## Payout rails, dates, and fees (plan of record — 2026-10-24)

Contributors asked for this in writing (discussion #295), so here it is.

**Fees: none, ever.** Participation costs nothing at any stage — no
registration, platform, verification, deposit, or prize-claim fee. Any party
asking contributors to pay to claim a bounty under this program's name is not
us; report it and we will flag it publicly.

**Amounts are USD-denominated** as listed in the tables above. They are not
converted or re-denominated at payout.

**Payout rail.** The leading candidate — driven by what contributors here have
actually asked for — is **USDC on Base to a self-custody wallet address you
control**. The final rail decision will be posted **in writing in discussion
#295 before the 2026-11-01 review window opens**, and this section will be
updated the moment it is confirmed. If you need a different rail for the
ladder tiers, say so in #295 before that date; we are reading it.

**Identity and tax documents.** Nothing is required to *submit* work. A rail
may impose its own requirements at payout time (for fiat rails, that usually
means KYC and tax paperwork); whatever applies will be stated in the rail
confirmation. Stablecoin self-custody has no identity requirement from us.

**Dates.** Triage is continuous — do not hold work waiting for rails. The
M1 wildcard verdict window opens **2026-11-01 00:00 UTC** (22 pre-registered
slots, discussion #365); verdicts post there, and payout preparation starts
after the last verdict of the window, per the published window rules.

**Eligibility is merit-based and location-agnostic** for submission and
review. Payout works wherever the confirmed rail works; if your jurisdiction
blocks the confirmed rail, contact us in #295 and we will work out an
alternative case by case.
