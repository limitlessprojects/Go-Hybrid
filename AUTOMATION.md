# Automation — how the PR backlog manages itself

Review-first automation. Two hard rules are encoded everywhere and must never
be relaxed:

1. **The bot never merges.** Landing requires the manual protocol
   (merge gate -> issue repro vs main -> `cargo test --workspace` ->
   `corpus_harness.py` -> `differential_loop.py` FULL) plus a credit commit.
2. **The bot never closes.** Closes are deliberate (credit-close with thanks,
   or wildcard verdict). `stale.yml` is label-only (`days-before-*-close: -1`).

## What runs where

| Piece | Where | Cadence | What it does |
|---|---|---|---|
| `tools/triage_bot.py scan` | local / agent sessions | on demand | Classifies every open PR: wildcard hold, draft, conflicts, stale-base, duplicate cluster, tests-included, new arrival, overlap-risk. Writes `TRIAGE.md` + `TRIAGE.json`. `--verify K` runs the worktree merge gate on the top-K land-candidates. |
| `tools/triage_bot.py approve` | local / agent sessions | on demand | Approves `action_required` CI runs for open PR heads (was ~190 manual clicks per session). Quota-guarded: `--max N`. `--dry` previews. |
| `tools/triage_bot.py labels` | local / agent sessions | on demand | Ensures the `triage:*` palette and applies per-PR labels (protects wildcard holds from staleness, flags conflicts/dups). |
| `tools/triage_bot.py board --post` | `.github/workflows/triage-board.yml` | daily 04:30 UTC + manual | Rewrites the **Triage Board** issue: Pareto NOW list, land-candidates, conflicts, stale bases, duplicate clusters, new arrivals, wildcard holds. |
| `actions/stale` | `.github/workflows/stale.yml` | daily 04:17 UTC | `triage:stale` label after 21 idle days. Never closes. Exempts `triage:wildcard-hold`, `triage:pareto`, `triage:keep`. |
| `ci.yml` | every PR | per push | Already runs the full gate suite: census, cargo tests, corpus verdict agreement, differential loop, Go runtime hygiene, examples. |

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
