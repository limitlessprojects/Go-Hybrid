# SHEPHERD — the automation layer (W0–W7)

Shepherd is the bot that absorbs every repetitive step so humans keep only the
judgment calls. `tools/triage_bot.py classify()` stays the **prior**; Shepherd
is layered on top of it and never replaces the verify protocol. Full design:
the plan adopted in session-25 ("Bot Improvement Plan — Shepherd").

**Division of labor:** the bot proposes, humans decide — except where a
narrow, provably safe rule exists (docs-only conflicts, CAS-guarded rebase,
push-and-assert). The two judgment calls that stay human *by design*:

1. **Stale-premise closes** (#804 class) — Shepherd W6 supplies probe
   evidence; a human signs the close (over-rejecting valid programs is a
   regression).
2. **Real-conflict extraction** (#620 class) — W2 rebases and W5 reports the
   diff summary; a human resolves.

## Kill switch (from day one)

Repo **variable** `SHEPHERD_MODE` (Settings → Secrets and variables → Actions
→ Variables) gates every workflow. Unset = **dry-run** (safe default).

| Mode | W1 board | W3 preflight | W7 log | W2 rebase | W4 Phase A | W4 Phase B | W5 verify |
|---|---|---|---|---|---|---|---|
| `off` | blocked | blocked | blocked | blocked | blocked | blocked | blocked |
| `dry-run` (default) | ✅ | ✅ | ✅ | report-only | ✅ | blocked | blocked |
| `live` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |

Check locally / in any shell: `python3 tools/shepherd.py gate --require live`.

## Label state machine

State lives in **labels**, not prose; the board (#940) is a *view* of that
state. `bot:*` labels (palette in `tools/shepherd_config.json`):

| Label | Meaning | Set by |
|---|---|---|
| `bot:land-candidate` | fresh, gate-clean, verifiable | classify() via W1 sync |
| `bot:conflict` | merge conflicts | classify() / W2 on failure |
| `bot:stale-base` | behind main beyond threshold | classify() via W1 sync |
| `bot:dup-of:#N` | cluster member superseded by #N | W4 Phase A |
| `bot:preflight-pass` / `-fail` | cheap-gate result | W3 |
| `bot:stale-premise` | premise contradicts pinned toolchain | W6 |
| `bot:verified` | full protocol green on the PR tree | W5 |
| `bot:ready-to-land` | human approved the merge | **human** (until Phase 6) |
| `bot:human-needed` | judgment call required | any workflow on failure |
| `bot:landed` | SHA confirmed on `origin/main` | W5 push-and-assert |

The `triage:*` palette (triage bot) continues unchanged; the two palettes
coexist (`bot:*` = machine state, `triage:*` = triage metadata).

## Workflows

| ID | Workflow | Trigger | Mode floor |
|---|---|---|---|
| W0 | `.github/actions/shepherd-setup` | composite, used by W3/W5 | — |
| W1 | `shepherd-board.yml` | PR/issue events + nightly 04:13 + dispatch | read |
| W2 | `shepherd-rebase.yml` | `*/20` cron + dispatch | live (report at dry-run) |
| W3 | `shepherd-preflight.yml` | `bot:land-candidate` labeled / synchronize | dry-run |
| W4 | `shepherd-dup.yml` | weekly Mon 03:41 + dispatch (`phase_b` input) | dry-run (B: live) |
| W5 | `shepherd-verify.yml` | `bot:ready-to-land` labeled + dispatch | **live** |
| W6 | inside W5 (probe) | verify start | live |
| W7 | `shepherd.py log` (step in every workflow) | end of every run | dry-run |

### The four fixes this layer exists for

1. **Board staleness < 5 min** — W1 rewrites #940 in place on PR events
   (self-trigger guarded: `github.actor != 'github-actions[bot]'`). Rate
   math: one scan ≈ 600 API calls; at ~50 events/day that is ≈1.2k calls/hr
   average against a 5k/hr budget. If event volume ever grows, drop
   `synchronize` from the W1 trigger first.
2. **Rebase without force-push** — W2 uses `PUT /pulls/N/update-branch` with
   `expected_head_sha` (compare-and-swap). A branch that moved under the bot
   is skipped, never overwritten. Conflicts → `bot:conflict` + evidence.
3. **Push-and-assert** — nothing is "landed" until
   `git merge-base --is-ancestor <pr-head> origin/main` passes on a fresh
   `ls-remote`. The `a4a9dab`/`b84fff6` incident class (commits claimed
   landed, not on any upstream branch) is now a loud failure that labels
   `bot:human-needed`.
4. **Structured log** — every run appends one JSON line to
   `logs/SESSION.jsonl` on branch `automation/session-log` via the contents
   API (CAS-retried). `python3 tools/shepherd.py log --event <e> --pr <n> ...`
   The log is a record; the board is the view.

## W6 stale-premise registry

`PROBES` in `tools/shepherd.py`: each rule has a diff pattern (the PR claims
a rejection), and a two-line probe compiled against the **pinned** toolchain.
Probe builds ⇒ premise is dead ⇒ `bot:stale-premise` + evidence comment +
auto-merge blocked. Human signs any close. Register a new rule when a verdict
turns on upstream behavior changing under us (the #804 lesson).

## Auto-merge criteria (deliberately narrow)

W5 merges only when ALL hold: `bot:preflight-pass` + full battery green +
not `bot:stale-premise` + no real conflicts (docs-only auto-resolved per the
#870 policy: keep main's PLAN.md/STATUS.md, keep the PR's code) + `bot:ready-to-land`
set + `SHEPHERD_MODE=live`. Merge commit carries `Original-author:` and
`Protocol:` trailers via `shepherd.py merge-msg`.

## Guardrails

| Control | Setting |
|---|---|
| Branch protection | bot merges via PR API only; no direct pushes to `main` (requires repo admin to enable — follow-up) |
| Token | workflows use `secrets.GITHUB_TOKEN` (contents/PR/issues write; **no admin**) |
| Concurrency | per-PR groups (`shepherd-verify-pr-N`), board/rebase/dup serialize globally |
| Rate caps | W2 ≤ 3 updates/hour (`max_per_run: 1` @ `*/20`); W1 scan ≈ 600 calls; per-tick ceilings in config |
| Kill switch | `SHEPHERD_MODE` (above) |
| Path policy | `owned_docs` (PLAN.md, STATUS.md) never taken from PRs |
| Audit | every action leaves a comment/label; W7 JSONL per run |

## Rollout (adopted plan §6)

- [x] Phase 0 — W0 + W7 shipped, `SHEPHERD_MODE` unset ⇒ dry-run
- [x] Phase 1 — W1 board + W7 logging (comment/edit only)
- [x] Phase 2 — W3 preflight on `bot:land-candidate` (comment-only)
- [x] Phase 3 — W2 rebase shepherd (CAS-guarded, capped)
- [x] Phase 4 — W4 dup closer: Phase A propose-only now; Phase B only after
      the mode variable is set to `live` **and** a survivor is `bot:verified`
- [x] Phase 5 — W6 probe shipped (blocks, never closes)
- [ ] Phase 6 — W5 auto-merge: ship code now, activation = set
      `SHEPHERD_MODE=live` + start setting `bot:ready-to-land` deliberately
- [ ] Phase 7 — widen auto-merge only if precision holds

## Success metrics (tracked from the W7 log + board)

- Board staleness < 5 min (was ~a day)
- % of PRs humans touch only after preflight pass
- Stale-base count trend (117 → down)
- Auto-merge precision: post-merge reverts per 100 landings, target 0
- Zero "landed but not on origin/main" incidents (structural now)
- Dup-cluster size shrink; false-close rate target 0
