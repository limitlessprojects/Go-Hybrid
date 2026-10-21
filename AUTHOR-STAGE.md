# Author stage — the layer that writes the fix (session-32, 2026-10-21)

Decision brief + v0 scaffold. Companions: `AUTOMATION.md` (the machine),
`SHEPHERD.md` (W0–W7), `automation/land-queue/README.md` (bundle format),
`AUTHOR-STAGE.md` governance summary in the workflow header.

## 1. Where the problems lie (2026-10-21 snapshot)

**112 open issues / 269 open PRs** (11 drafts). Issue titles classified by
keyword, first-match (a distribution map, not per-issue truth — the
classifier itself misfiled #1101, a tooling issue, via the substring
"tail"; `[wildcard]` / `[tooling]` proposals are out of author-stage
scope regardless of class):

| bug class | issues | share |
|---|---|---|
| const eval / overflow | 39 | 34.8% |
| bounds / index / slice | 32 | 28.6% |
| closure / alias / loan / ownership | 16 | 14.3% |
| (T, error) / multi-value propagation | 8 | 7.1% |
| string / rune / fmt | 6 | 5.4% |
| channel / spawn / select | 4 | 3.6% |
| map / dict | 4 | 3.6% |
| build / emit / import / link | 2 | 1.8% |
| other | 1 | 0.9% |

**63.4% of the open bug queue is the two classes the #998/#999 landings
just built fix templates for** (named-const folding via the file const map
in `const_overflow.rs`; `place_as_expr` base propagation in
`buildable.rs`). That is where an author agent has the highest prior of
producing a correct bundle — and where the mechanical tripwires (accept
corpus + census + count surfaces) catch a wrong rule. The PR backlog is a
different problem: 148/269 are `bot:stale-base`, 61 conflicts, 45+25+10
in duplicate clusters — land-side triage the shepherd already covers.
**The author stage must never touch claimed PRs** (contributor credit is
non-negotiable); it authors fixes for *unclaimed issues* only.

Evidence: `reports/assets/queue-taxonomy-2026-10-21.json` +
`scripts/queue-taxonomy-1021.py` (ghost-fork repo).

## 2. Brain decision — Ornith-1.0-9B

Verified against primary sources (2026-10-21):

- **Real**: HF `ornith-ai/Ornith-1.0-9B` + `Ornith-1.0-9B-GGUF` — MIT
  license, qwen3_5 architecture, 256K context, Q4_K_M = 5.63 GB,
  ~2.37M downloads/month; GitHub `ornith-ai/Ornith-1` (MIT); Kaggle
  mirrors confirmed via `kaggle models list -s ornith`.
- **The 69.4 SWE-bench Verified claim is provider-reported** (DeepReinforce
  model cards; third-party posts repeat the card). benchlm.ai: "tracked,
  but not publicly ranked" — all 7 published rows are provider-sourced;
  no independent leaderboard row exists. SWE-bench Pro 42.9% for the 9B.

**Verdict: bake-off candidate, not the default brain.** Same evidence bar
we apply to our own engine: a benchmark number never reproduced on OUR
distribution (a Rust checker + .hy corpus, not Python repos) is NOT RUN,
never PASS. Hardware reality: CPU-only runners are ruled out by
throughput (an authoring loop needs ~10^5 tokens; ~10 tok/s CPU = hours
of dead air per attempt). The free Kaggle GPU tier (P100/T4, 16 GB) runs
Q4_K_M or Q8_0 comfortably (~30–60 min/bundle estimated), and this repo
already dispatches Kaggle kernels (`kaggle.yml` + `tools/job.py kaggle
submit/wait/fetch`) — hosting a brain there reuses existing pipeline,
$0 compute. **Ornith-1.5-9B** (newer self-improvement generation, GGUF
available) joins the bake-off as a second candidate; it costs nothing
extra to try on the same runner.

## 3. Harness decision — vex-harness vs mini-SWE-agent vs ours

Due diligence (PyPI/GitHub, 2026-10-21):

- **vex-harness**: real (PyPI 200) but **v0.2.0, two releases
  (2026-09-12 → 2026-09-22), single author**, pins `litellm==1.74.9`
  (wide transitive surface). Its architecture (snapshot → plan → edit →
  verify → branch + trace) matches our need, but one month of history is
  not enough to grant it the push path or our tokens. Role for now:
  **observe-mode bake-off candidate, no credentials, no push path.**
- **mini-swe-agent**: mature, auditable (~100 lines, bash-only tool),
  PyPI live — the right third-party loop if we ever want one.
- **ours**: the hard part (verification) already exists as the autoland
  gates; the author layer only needs to emit a bundle.

**Verdict: the default author harness is `tools/author_stage.py` v0**
(~330 auditable lines, zero third-party deps beyond stdlib). vex-harness
may compete in a later round without ever holding a credential.

## 4. Architecture — the author writes bundles; autoland stays the only path to main

```
issue #N ──> tools/author_stage.py author --issue N
                 │  brain = any OpenAI-compatible endpoint:
                 │  llama.cpp + Ornith GGUF on a Kaggle GPU kernel,
                 │  or a cloud model (secrets AUTHOR_API_*)
                 ▼
         automation/land-queue/author-<N>/
           meta.json  fix.patch  repro.hy  trace.log
                 ▼   (READY bundles only — BLOCKED never enters
                 ▼    the queue; blocked evidence = artifacts only)
         autoland.yml gates (UNCHANGED):
           baseline ACCEPT → patched REJECT w/ expected code
           → full battery → push-and-assert → JSONL change log
```

Kill-switch ladder (each stage one step behind the next):

| switch | default | unlocks |
|---|---|---|
| `AUTHOR_ENABLED` (repo var) | unset | the author-stage workflow at all |
| `AUTHOR_MODE` (repo var) | `dry-run` | `live` = commit the READY bundle → autoland (dry-run) gates it publicly |
| `SHEPHERD_MODE` (unchanged) | `dry-run` | `live` = autoland actually lands |

Hard rules encoded in `tools/author_stage.py`, not prose: never pushes
code, never lands, never closes; refuses a dirty tree; scope-guarded
diffs (`compiler/` + `tests/` + `automation/land-queue/` only); PRs
refused outright; trace.log records every prompt, completion size, and
gate exit code; MAX_ROUNDS exhaustion = BLOCKED with the last failure
verbatim; over-reach tripwires read accept-controls from HEAD so the
patch cannot weaken them.

## 5. Bake-off round 1 (protocol — gates frozen before any run)

Scope: **3 unclaimed `soundness(hy-check)` issues from the top-2 classes**
(no `[wildcard]`/`[tooling]` prefixes; smallest self-contained bodies;
the final pick is recorded in the run log BEFORE any brain is invoked).
Brains: `ornith-1.0-9B@kaggle` · `ornith-1.5-9B@kaggle` · one cloud model
via the same OpenAI-compatible seam.

Judge (all pre-declared; anti-Goodhart: **no gate, harness scoring, or
battery change during the bake-off**; results recorded even when ugly):

1. bundle→READY rate within ≤4 rounds
2. autoland gates: baseline ACCEPT / patched REJECT + expected code
3. full battery green on the bundle
4. over-reach: accept corpus + census clean
5. Ghost-Fork P1 on the bundle's coupled surface (our extra dimension —
   predictions seeded before authoring, per the round-6 study protocol)

Human baseline (not a straw man): the sessions' last 10 landings are
10/10 battery-green with 0 post-landing reverts. A bot that lands 2/3
with one over-reach escape is WORSE than the human bar — the report says
so in those words if it happens.

## 6. Scoreboard — "beats humans" is measured, not felt

- **latency**: issue→landed (bot: minutes–hours; human sessions: days)
- **gate-green rate** + post-landing regression rate (CI + corpus tripwire)
- **over-reach escapes** (accept-corpus breakage) = hard negative
- **Ghost-Fork P1/P2** recorded per landing (a dimension humans never measured)
- **cost per landed fix** (Kaggle = $0 compute; cloud = $/bundle)

## 7. Shipped today / not shipped

Shipped: `tools/author_stage.py` (v0, selftest green, stdlib-only),
`.github/workflows/author-stage.yml` (dispatch-only, three-switch
ladder), this brief, the `AUTOMATION.md` pointer, the queue taxonomy
evidence.

Not shipped (deliberate): brain endpoints — `AUTHOR_API_BASE` /
`AUTHOR_API_KEY` / `AUTHOR_MODEL` secrets must be provisioned before any
run; the Kaggle author kernel (round-1 infrastructure, next session); any
bake-off result. Honest status of the author layer itself: **unvalidated
plumbing until bake-off round 1 runs — the same posture Ghost-Fork had
after M10, and it earns trust the same way.**
