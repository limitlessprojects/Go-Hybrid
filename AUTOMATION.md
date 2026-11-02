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
