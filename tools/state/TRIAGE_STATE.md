# TRIAGE STATE — cold-start entry point

If you are a fresh agent reading this after a workspace wipe: you do not need
conversation history. Everything needed to resume is here + in the scripts.

## Resume procedure
1. Token: `base64 -d /home/z/my-project/scripts/token.b64` (auto-handled by scripts).
2. Run `bash /home/z/my-project/scripts/fired_tick.sh` — it bootstraps, then runs one
   bounded tick (`triage_tick.sh --max 60`) and appends evidence to `heartbeat.log`.
3. Read the latest entries in `tools/state/heartbeat.log` (this file) for where things stand.

## Scheduler recipe (what finally worked — teachable)
The z.ai cron wake works when the fired payload is TINY and all intelligence lives
outside the conversation:
- Cron task message is a few lines max: "run this one script, reply with its SUMMARY
  lines (max 8), stop if TOKEN DEAD/MISSING". No task description in the payload.
- The script (`fired_tick.sh`) is self-bootstrapping: recovers the token from durable
  slots, re-clones/restores the repo, restores the queue snapshot from this very
  directory (repo-carried state), runs ONE idempotent capped tick, pushes a heartbeat
  line back here (externally observable proof the firing happened), prints a 1-line summary.
- Idempotency makes duplicate/missed firings harmless: every close re-verifies live
  state before acting; already-closed PRs are skipped.
- Evidence discipline: every run logs to `scripts/fired_runs/<ts>.log` and heartbeats
  here. A firing with zero output = something broke silently (past lesson: dead token).

## Guard doctrine (quality-first, do not regress)
- NEVER merge. NEVER close CI-green or in-flight PRs. Closes need age >= 5d PLUS
  hard evidence, re-verified LIVE at close time:
  A. >=1 NON-board check-run concluded FAILURE (the repo's own shepherd/triage board
     jobs are excluded — they get concurrency-cancelled and read as FAILURE; this
     caused 3 wrongful closes on 2026-11-27, reopened 2026-11-28).
  B. CONFLICTING merge state + idle >= 10d (never applied to green CI).
- Vetoes: protected labels (triage:wildcard-hold/pareto/keep), `bot:land-candidate`
  (board says green), config holds/parked drafts, wildcard titles, reopened <= 5d,
  author commented <= 3d. Batch cap per tick (~40-60). Every close gets a written
  reply with the actual failing job names and a reopen path.
- Known truth: GitHub silently ignores `state_reason` on PULL REQUESTS (issues only).
  Do not chase it, do not reopen/reclose to set it — that flapping made things worse.
- Land gate: green CI + CLEAN/BEHIND -> land_candidates.json (BEHIND = needs branch
  update). Landing is human-only.

## Standing instruction from the maintainer (2026-11-28)
Use MULTIPLE PARALLEL SUBAGENTS each session to cover more ground (survey / QC audit /
verification), while close/merge actions stay centralized in the guarded runner.
Work slowly and deliberately; the closable pool is small and guard-gated on purpose.

## Next actions
- #4677, #4678 cross MIN_AGE on 2026-11-28 (verify author-comment silence >= 3d, then
  live non-board CI evidence, then close with reply).
- #4469 closeable after 2026-11-29T12:13Z if author wminno stays silent.
- ~13 wildcard-title PRs are permanently HUMAN (M1 doctrine) — never auto-close.
- Conflict-idle gate is mostly dormant: bot labeling resets updatedAt (idle clock
  pollution) — author-comment idleness is the honest signal.
- 2026-11-28: docs hygiene pass landed in opencode-config (two commits). No further
  action required there; maintainer holds the private checklist.
