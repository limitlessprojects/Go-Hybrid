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
