# Automation — how the PR backlog manages itself

Review-first automation. One hard rule is encoded everywhere and must never
be relaxed, plus one policy that keeps the queue honest:

1. **The bot never merges.** Landing requires the manual protocol
   (merge gate -> issue repro vs main -> `cargo test --workspace` ->
   `corpus_harness.py` -> `differential_loop.py` FULL) plus a credit commit.
2. **The queue drains itself** (doctrine change, 2026-10-24). The old rule —
   "the bot never closes" — was written for a repo receiving a handful of
   PRs; at ~65 arrivals/day it produced a 400+ PR review-debt pile that no
   session could honestly work through. Now `tools/queue_sweeper.py` (daily
   via `.github/workflows/queue-sweeper.yml`) closes unprotected PRs older
   than `SWEEPER_MIN_AGE_DAYS` (default 5) as not-planned, with a written
   reply on every close (the CONTRIBUTING "no silent closes" promise is
   unchanged). A PR survives only with a protected label —
   `triage:wildcard-hold`, `triage:pareto`, or `triage:keep` — or by being
   listed in `triage_config.json` (wildcard holds, parked drafts). The daily
   triage board is what earns that protection: if a PR deserves review time,
   it must surface within its window. Closes are reopen-friendly by design;
   merges stay human.

## What runs where

| Piece | Where | Cadence | What it does |
|---|---|---|---|
| `tools/triage_bot.py scan` | local / agent sessions | on demand | Classifies every open PR: wildcard hold, draft, conflicts, stale-base, duplicate cluster, tests-included, new arrival, overlap-risk. Writes `TRIAGE.md` + `TRIAGE.json`. `--verify K` runs the worktree merge gate on the top-K land-candidates. |
| `tools/triage_bot.py approve` | local / agent sessions | on demand | Approves `action_required` CI runs for open PR heads (was ~190 manual clicks per session). Quota-guarded: `--max N`. `--dry` previews. |
| `tools/triage_bot.py labels` | local / agent sessions | on demand | Ensures the `triage:*` palette and applies per-PR labels (protects wildcard holds from staleness, flags conflicts/dups). |
| `tools/triage_bot.py board --post` | `.github/workflows/triage-board.yml` | daily 04:30 UTC + manual | Rewrites the **Triage Board** issue: Pareto NOW list, land-candidates, conflicts, stale bases, duplicate clusters, new arrivals, wildcard holds. |
| `actions/stale` | `.github/workflows/stale.yml` | daily 04:17 UTC | `triage:stale` label after 21 idle days. Never closes. Exempts `triage:wildcard-hold`, `triage:pareto`, `triage:keep`. |
| `ci.yml` | every PR | per push | Already runs the full gate suite: census, cargo tests, corpus verdict agreement, differential loop, Go runtime hygiene, examples. |
| `tools/queue_sweeper.py` | `.github/workflows/queue-sweeper.yml` | daily 05:15 UTC + manual | Closes unprotected PRs older than `SWEEPER_MIN_AGE_DAYS` (default 5) as not-planned, each with a reopen-path reply. Protected: the `triage:wildcard-hold` / `triage:pareto` / `triage:keep` labels plus config-listed wildcard/parked numbers. |
| `tools/queue_sweeper.py --dry` | any run | before changes | Preview exactly what the next sweep would close — same policy, zero writes. |

## Session start (the 30-second context restore)

Board posting uses `board_issue` in `tools/triage_config.json` when configured;
this repository pins #940. The target must remain an open issue with the exact
board marker. Without that setting, discovery requires the exact board title
and marker plus a repository owner/member/collaborator or GitHub Actions author.
An incomplete search or multiple eligible boards stops without writing. With
no eligible board, the existing first-run workflow creates one.

```bash
python3 tools/triage_bot.py scan --out /tmp/triage        # fresh queue
python3 tools/triage_bot.py approve --max 40              # unblock CI
```

Then read `/tmp/triage/TRIAGE.md` (or the Triage Board issue — same content,
updated daily). Work the **Pareto NOW** list top-down; everything else is
pre-classified so recon is only needed where the bot says `RECON`.

## Patches welcome, verdicts human

The bot classifies with cheap signals (API mergeability, commit distance,
file-set overlap). It cannot detect silent hunk deletion, semantic breakage
after clean API merges, or subsumption — those stay on the verify protocol.
Treat every bot verdict as a *prior*, not a decision.

## Later options (not enabled — need human review)

- **GitHub native merge queue** (free for public repos, needs branch
  protection + required checks): would replace hand-merging for PRs that pass
  everything. Deferred — credit messages must stay hand-written.
- **GitHub native auto-merge**: same blockers as above.

## Config

`tools/triage_config.json`: wildcard holds (M1 experiment, verdicts due
2026-11-01), parked drafts, thresholds, label palette. Keep
`wildcard_holds` sorted; the loader asserts it.

## Shepherd (session-25): the automation layer on top of this bot

`tools/shepherd.py` + the `shepherd-*` workflows (W0–W7) absorb the
repetitive steps: live board refresh (W1), CAS-guarded rebase (W2), cheap
preflight gates (W3), duplicate-cluster proposals (W4), verify-and-land
mechanics with push-and-assert (W5), stale-premise probe (W6), structured
JSONL session log (W7). Kill switch: the `SHEPHERD_MODE` repo variable —
unset means **dry-run** (read/comment/label only); mutations unlock at
`live`. The two judgment calls stay human by design: stale-premise closes
and real-conflict extraction. Full manual: **SHEPHERD.md**.

## Autoland (session-31) — prepared fix bundles land themselves

`tools/autoland.py` + the `autoland` workflow close the loop for the landing
queue: a fix bundle at `automation/land-queue/<slug>/` (meta.json + fix.patch
+ repro.hy) is verified end-to-end and landed with no human awake — baseline
ACCEPT → patch → patched REJECT with the expected code → full battery →
commit (`Fixes #N`) → push-and-assert LAND-CONFIRMED → bundle consumed →
shepherd log. Any gate failure records BLOCKED with evidence and lands
nothing. Same kill switch as the shepherd: `SHEPHERD_MODE=live` unlocks
landing; the default `dry-run` runs baseline checks only. Format, gates, and
trust boundaries: `automation/land-queue/README.md`. The judgment work
(authoring the fix + its tests + the corpus registration) stays with the
agent sessions; everything mechanical after that is the machine's.

## Author stage (session-32) — the layer that writes the fix

`tools/author_stage.py` + the `author-stage` workflow produce a land-queue
bundle from an issue (brain = any OpenAI-compatible endpoint, e.g. an
Ornith-1.0-9B GGUF llama.cpp server on a Kaggle GPU kernel, or a cloud
model). It never pushes code, never lands, never closes: a READY bundle
enters `automation/land-queue/` and is gated by the SAME autoland
pipeline; BLOCKED bundles never enter the queue (artifacts only, full
trace). Scope-guarded diffs (`compiler/` + `tests/` only), accept
controls read from HEAD so a patch cannot weaken its own tripwire, PRs
refused outright. Kill switches stack in front of autoland's:
`AUTHOR_ENABLED` arms the workflow, `AUTHOR_MODE=dry-run` (default)
keeps everything to artifacts + evidence comments, `SHEPHERD_MODE`
(unchanged) still guards the landing. Brain/harness decisions, the
problem taxonomy (63.4% of open bugs = const-eval + bounds), and the
pre-declared bake-off protocol: **AUTHOR-STAGE.md**.

## Deliberate close runner (session-188; on GitHub Actions since 2026-11-26)

The retired sweep's replacement for the red-CI drain: `tools/pr_triage_runner.py`
(+ `tools/queue_snapshot.py` for the GraphQL survey). Doctrine unchanged — the
bot never merges, closes are deliberate, no silent closes. A PR is closed as
not_planned only if ALL of:

  - it is unprotected (no hold labels, not in `triage_config.json`),
  - age >= `TRIAGE_MIN_AGE_DAYS` (default 5),
  - hard evidence: CI rollup FAILURE (or CONFLICTING + idle >= 10d),
  - it is not wildcard-titled (catches unlabeled M1 submissions),
  - its author has not commented in the last 3 days,
  - it was not reopened inside `TRIAGE_REOPEN_WINDOW_DAYS` (default 5).

Green-CI and no-CI PRs are NEVER closed; green-CI + CLEAN PRs are exported as
land candidates (`$TRIAGE_STATE_DIR/land_candidates.json`) for a landing
session. Every close carries a PR-specific evidence reply with the reopen
path, and `state_reason: not_planned` is set via the issues API (the
pulls-API PATCH silently drops the reason).

**Where it runs now.** The agent-workspace cron (z.ai) has a dead dispatch
layer — jobs 540028 (~980-char payload, 84 firings) and 545580 (~210-char
payload, 7 firings) both produced zero dispatch effect, so payload length is
ruled out — and since 2026-11-26 the same runner file therefore drives
`.github/workflows/deliberate-close.yml` on GitHub's own scheduler: every
6 hours at :23 UTC plus manual dispatch (`dry` input defaults true, `max`
default 40). Each run re-uses the carried snapshot in
`tools/state/queue_snapshot.json` when it is < 24 h old (else re-surveys via
`tools/queue_snapshot.py`), dry-run gates the batch, and commits a heartbeat
line + fresh snapshot back to `tools/state/` — that `automation:` commit is
the externally visible evidence a run happened. First validation run
(#45785005646) succeeded end-to-end on 2026-11-26; session 194 added a
`!tools/state/heartbeat.log` negation to `.gitignore` (the root `*.log`
rule was silently excluding the heartbeat line, so evidence commits never
landed) and dispatched run #45812557658 validated the full write path
(4 closes, each with its evidence reply).

**State commits live on the `automation/state` branch (session 195).** main
is branch-protected and the classic protection API offers no bypass for the
Actions app on a personal (non-org) repo: run #45856859601 closed 7 PRs and
then lost its heartbeat to `failed to push some refs`, and the PATCH that
would grant a bypass 422s on `restrictions` whichever way it is expressed
(`null`, `{}`, full-empty all rejected). So the runner force-pushes its
heartbeat + snapshot + land-candidate export to
`refs/heads/automation/state` instead, and the survey step prefers a <24 h
copy from that branch, then main's carried copy, else surveys fresh. Dry run
#45858411583 validated the route end-to-end (heartbeat commit 92340766 on
the branch). The tick no longer depends on owner pushes for snapshot
freshness; agent sessions may still refresh main's carried copy.

**Mergeability is REST-fixed at survey time (session 195).** GitHub computes
mergeability lazily, so the bulk GraphQL survey recorded
`mergeStateStatus: UNKNOWN` for ~1,020 open PRs and the land-candidate
export stayed empty forever. The surveyor now walks the green-CI +
non-draft + hold-free subset with individual REST GETs (which return the
real value on first fetch) and overwrites `mergeable`/`mergeStateStatus`;
rate limits stop the walk early and affected PRs keep their survey values.
First pass over 132 candidates: **7 land candidates (clean)**, 124 behind,
0 dirty, 0 unknown.

Usage (any agent session; token required for writes, the survey is read-only):

```bash
export TRIAGE_STATE_DIR=/tmp/gh-triage
python3 tools/queue_snapshot.py                       # GraphQL survey, ~4 min
python3 tools/pr_triage_runner.py --dry --max 40      # review decisions first
python3 tools/pr_triage_runner.py --execute --max 40  # capped batch, idempotent
```

Runs are bounded (REST budget guard, rate-limit sleeps) and safely
re-runnable: already-closed, reopened, author-active and protected PRs are
re-checked live and skipped, so a stale snapshot never double-closes.
