# MOONSHOT — the Wildcard Track ledger

**This file is the public home of the Wildcard Track**: the $500-per-cycle
bounty for revolutionary, contrarian, high-risk ideas that a normal feature
bounty would reject (rules and payout split in
[BOUNTY.md](BOUNTY.md) Track 3; triage and ops run by the project's AI
maintainer, with the maintainer keeping the final say and every decision
getting a written reason — same as the ladder).

Its purpose: **breakthroughs, not incremental improvements.** No working
prototype is required — this is purely a place for us to analyze people's
crazy ideas and thoughts. Breaking changes, new paradigms, rewrites, and
"impossible" ideas are explicitly allowed. **Failure is a valid outcome if
it produces knowledge — and every outcome, including every failure, is
published here.** We are hunting for the unique approaches or additions
that make a paradigm shift without breaking everything; where the
cost-to-ROI line sits is our call, and we say so in writing, per entry.

---

## How an idea moves through this file

```
received → triaged (≤48h) → scheduled → in-review → M1 accepted ($250)
        → in-progress → M2 learning published ($250) → paid
        ↘ declined (reason published)   ↘ failed (post-mortem published, M2 paid)
```

- **Intake:** open an issue with the
  [`wildcard` template](https://github.com/limitlessprojects/Go-Hybrid/issues/new?template=wildcard.yml);
  it auto-labels `wildcard` + `💎 Bounty`. Triage within 48 hours: entered
  into the ledger below, or declined with a published reason (a good
  decline is itself a learning — see the wall).
- **Review scheduling:** a public review window opens on the **1st of each
  month** in Discussions; every ledger entry gets a slot. P0-class
  soundness insights that fall out of a proposal bypass the calendar and
  follow the GUARANTEES.md protocol.
- **Milestones:** every entry carries named, dated milestones; the ledger
  is the single source of truth and is updated on every state change.
- **Payouts:** **$250 on acceptance (M1) + $250 when the learning is
  published (M2) — success or failure alike.** One wildcard winner per
  cycle; a learning that publishes in a later cycle rides that cycle's
  budget. Winners are announced in release notes and the Hall of Fame,
  same as the ladder.
- **Solicitation is active, not passive:** the calendar below names the
  assumptions we want attacked *this cycle*. Don't wait for an invitation
  that matches your idea exactly — attack adjacent assumptions too.

---

## Ledger

| ID | Proposal (one line) | Author | Source | Status | Review scheduled | Milestones | Payout | Outcome |
|----|---------------------|--------|--------|--------|------------------|------------|--------|---------|
| W-000 | *Track founded 2026-10-15* — owner decision adding the Wildcard Track ($500/cycle) to the bounty program; ops delegated to the AI maintainer | owner | BOUNTY.md Track 3 | **M1 accepted** | 2026-11-01 window | M1 done · M2 = this file | $250 (logged) | track live |
| W-001 | Kill syntactic alias/loan/freeze tracking: Linear Region Capabilities `Cap[R]` + type-colored sub-graphs replace path-sensitive value-flow (family head) | ajithnair1808-max | #304 | **triaged: accepted** | 2026-11-01 · slot 1 (+ spike PR #336; dup-credit #175) | M1 2026-11-01 · M2 learning | — | — |
| W-002 | Delete the alias graph: ownership as a capability the type carries (`own`/`share`/`borrow` constructors; may_share is too coarse in both directions) | yaojiajia838-design | #279 | **triaged: accepted** | 2026-11-01 · slot 2 | M1 2026-11-01 · M2 learning | — | — |
| W-003 | Linear capability types `own[T]`/`share[T]`/`view[T]` + one runtime ownership bit per composite header (static + runtime defense in depth) | zidirahim617-del | #193 | **triaged: accepted** | 2026-11-01 · slot 3 | M1 2026-11-01 · M2 learning | — | — |
| W-004 | Zero-checker emit: phantom capability witnesses in emitted Go; `cmd/compile` enforces use-after-move (checker becomes witness producer) | area523ufo | #174 | **triaged: accepted** | 2026-11-01 · slot 4 | M1 2026-11-01 · M2 learning (+ spike PR #507 — maintainers, session-18: witnesses 0 allocs/op = baseline, P4 falsifier fired: captures still share storage; converges with #370 P4) | — |
| W-005 | Clone mutable object graphs at task and channel boundaries (message isolation; memo-table preserves cycles inside the copy) | nikolas-sapa | #342 | **triaged: accepted** | 2026-11-01 · slot 5 (dup-credit #308) | M1 2026-11-01 · M2 learning | spike landed: PR #412 (race-verified; verdict CONDITIONAL — correctness proven, 2x spawn budget fails ~40,000x at 10k nodes; narrowings: opt-in isolation / bounded mailboxes / COW) | — |
| W-006 | Cancellation-safety as a one-pass dataflow corollary of Tier-0; flip GUARANTEES row 12 to a checked-subset claim | raju12345-raju | #352 | **triaged: accepted** | 2026-11-01 · slot 6 (row-12 doc pointer: PR #474 (pedromineo); no executable spike yet) | M1 2026-11-01 · M2 learning | — | — |
| W-007 | Certify cancellation-safe structured task regions through blocking-effect admission (opt-in region, compositional summaries) | yupengtang | #348 | **triaged: accepted** | 2026-11-01 · slot 7 (+ spike PR #405 (Rohank143); analysis-only PR #446 (pedromineo)) | M1 2026-11-01 · M2 learning | — | — |
| W-008 | Cancellation is not completion: checked settlement for own-handle task regions (terminal-outcome accounting per handle) | luvskyy | #257 | **triaged: accepted** | 2026-11-01 · slot 8 (+ new spike PR #569 (hardik4love-lang), 2026-10-18 — finite child model, not yet maintainer-verified) | M1 2026-11-01 · M2 learning | — | — |
| W-009 | Affine `tx[T]`/`rx[T]` endpoint split + wait-for epochs: row 16 flips via O5-consuming `close(move tx)`, E3201 cycle rejection | lienhoatienphung-svg | #347 | **triaged: accepted** | 2026-11-01 · slot 9 (+ spike PR #452 (Rohank143)) | M1 2026-11-01 · M2 learning | — | — |
| W-010 | Generate closed channel protocols from one global program (session-type endpoint generation; deadlock freedom in a closed subset) | elliotthugftgo86r66 | #184 | **triaged: accepted** | 2026-11-01 · slot 10 | M1 2026-11-01 · M2 learning | — | — |
| W-011 | Erase the `.hy`/`.go` split: whole-package Tier-0 on native Go AST + export data (directives-on-Go, `toolexec`) | CarlitoDon | #186 | **triaged: accepted** | 2026-11-01 · slot 11 | M1 2026-11-01 · M2 learning | — | — |
| W-012 | Self-hosted in-process analysis: `x/tools/go/analysis` driver for gopls/vet/WASM delivery + live typed symbol tables | 1932819331hy-debug | #268 | **triaged: accepted** | 2026-11-01 · slot 12 (co-reviewed with W-011) | M1 2026-11-01 · M2 learning (+ fork artifact: branch spike/w012-inprocess-analyzer, prereg 58ad41ae/closeout 937378d7, WASM parity 55/55, warm p50 0.269ms vs native 5.038ms ≈ 18.7x — no upstream PR; + hardening spike PR #556 (1932819331hy-debug), 2026-10-18, not yet maintainer-verified) | — |
| W-013 | Make hy-emit untrusted: translation validation of the actual generated Go against a checked-effect IR (certificates are hints) | MyTH-zyxeon | #345 | **triaged: accepted** | 2026-11-01 · slot 13 | M1 2026-11-01 · M2 learning | — | — |
| W-014 | HyRegion: task-owned off-heap linear slabs + 1-bit region unification; kills the alias graph AND the 758 ns handle-table wall | yodakkak | #239 | **triaged: accepted** | 2026-11-01 · slot 14 (+ spike PR #381 (yodakkak)) | M1 2026-11-01 · M2 learning | — | — |
| W-015 | Lexical split-join slice capabilities via Go 3-index capacity clipping (`split` block → `s[:mid:mid]`; provable disjoint fork-join) | michellehumes | #294 | **triaged: accepted** | 2026-11-01 · slot 15 | M1 2026-11-01 · M2 learning | spike landed: PR #379 (race-verified — `split` syntax, E3105/E3106, corpus 53, capacity-clip disjointness) | — |
| W-016 | A5 Go-escape as check-site with no Go API database: capture-write at a foreign-call boundary takes `managed` | airdropia | #327 | **triaged: accepted** | 2026-11-01 · slot 16 (reference implementation: PR #464 (pedromineo) — un-endorsed, self-landed ledger edits rejected) | M1 2026-11-01 · M2 learning | — | — |
| W-017 | Operational take-and-detach move lowering (runtime-enforced ownership); bridge-deletion half carries a scope note | sonson0910 | #200 | **triaged: accepted** | 2026-11-01 · slot 17 (+ spike PR #370 (sonson0910)) | M1 2026-11-01 · M2 learning | — | — |
| W-018 | Compile closed fixed-rate task networks away: certified periodic Go kernels with explicit ring-buffer budgets (SDF schedule certificates; bounded FIFO storage, no channel-token deadlock in the admitted subset) | untalsix | #378 | **triaged: accepted** | 2026-11-01 · slot 18 (spike PRs #384 (Anekenonso, SDF planner + certificates), #480 (sdf_cert.py generator + independent replay-only checker), and #572 (Rohank143, 2026-10-18 — runtime FIFO guards, not yet maintainer-verified), prototype credit untalsix) | M1 2026-11-01 · M2 learning | — | — |
| W-019 | Isolated WASM task cells: replace cross-task alias proofs with a wire boundary (serialized message passing across a WASM edge makes the alias graph local per cell) | Rangercook | #462 | **intake acked 2026-10-18** (48h window) | verdict slot: M1 if artifact lands in time, else December window (+ spike PR #566 (gttps), 2026-10-18 — M1/M2 executable spike, not yet maintainer-verified) | M1 2026-11-01 · M2 learning | — | — |
| W-020 | Serializable shared-state regions via admitted transactions (STM-flavored regions; author's bounded experiment is self-declared unreviewed) | matheuswallace | #417 | **intake acked 2026-10-18** (48h window) | verdict slot: M1 if artifact lands in time, else December window | M1 2026-11-01 · M2 learning | — | — |
| W-021 | Kill libhyrt.a for SMALL/DEFAULT via Go-ABI0 asm + static stack certificates (NOSPLIT leaves); spike #488 falsifies direct asm 16/19 exports and reports 3 live descriptors.md stack hazards | alejandrobrcca-s | #375 | **triaged: P0-adjacent fast-track** (descriptors hazards verified out-of-band; spike verdicts at M1) | 2026-11-01 · slot 19 (+ spike PR #488) | M1 2026-11-01 · M2 learning | — | — |
| W-022 | Linear Synchronization Capsules flip GUARANTEES rows 8, 10, 11: `AtomicCapsule[T]`/`LinearPool[T]`/`MutexCapsule[T]` as Tier-0 checked capabilities (O5.2 move-freeze witnesses replace the `managed("…","mutex"|"atomic")` string permits; falsifier P4a fired as predicted — third independent convergence on the phantom-witness/may_share boundary, after #370 and #507) | carelessdept | #544 | **intake acked 2026-10-18** (48h window) | verdict slot: active 2026-11-01 window — late intake, reviewed with the drafted slots (spike PR #545: race-verified, preregistered, scoped to spikes/w022-linear-sync-capsules/; requested strengthening delivered in a7ba19f — 3 minimal .hy repros, before/after row text, P4a remedy path); December window if review incomplete | M1 2026-11-01 · M2 learning | — | — |
| W-023 | Make the Go toolchain the oracle (INVARIANT 4: every checker-accepted program must build): seeded Go-decidable generator + corpus audit + oracle-build, extending tools/fuzz/hyfuzz.py behind opt-in flags; first run found the C1–C10 classes (now the landed BUILDABLE sweep), maintainer re-run (same seed, main d7ab63a) shows residual 38.03% → 2.78% with the last 2 shapes filed as #998/#999 | Khanhtnq | #565 | **intake acked + maintainer review completed 2026-10-20** (one check_one error-path defect found in review, fix named in the ack) | 2026-11-01 · slot 22 (+ spike PR #577) | M1 2026-11-01 · M2 learning | — | — |
| W-031 | The rule-mutation score: measure what the corpus can actually detect, and gate every GUARANTEES.md row on a killed mutant | Dimany4 | #2198 | **intake acked** | 2026-11-01 · slot 30 (+ spike w031-rule-mutation-score) | M1 2026-11-01 · M2 learning | — | — |

*First-window harvest (triaged 2026-10-17, all within the 48-hour acknowledgment
commitment): 19 submissions → 17 ledger entries + 2 duplicate-credits (#175 →
W-001 family, #308 → W-005). Review family grouping for the window: **capabilities
replace the alias graph** (W-001–W-004, W-014, W-017), **message isolation**
(W-005), **cancellation & protocol liveness** (W-006–W-010), **toolchain rewrites**
(W-011–W-013), **targeted capabilities** (W-015, W-016). The 2026-11-01 window
thread links every slot; M1 verdicts land there with written reasons.*

## Review windows

| Window | Entries | Thread | Outcome |
|--------|---------|--------|---------|
| 2026-11-01 (opens 00:00 UTC) | W-001 … W-031 (W-022 = late intake 2026-10-18; W-023 = late intake, review completed 2026-10-20; W-031 = intake #2198), spike PRs #336 #370 #379 #381 #384 #405 #412 #452 #480 #488 #545 #577 (+ #446/#464/#474 as auxiliary inputs; late-arriving spikes #556 #566 #569 #572 under verification) | [#365 — slots assigned, authors invited to pre-answer](https://github.com/limitlessprojects/Go-Hybrid/discussions/365) | pending |

---

## Published learnings (all outcomes — successes and failures)

Format per entry: **Proposal → Verdict → What we learned → Cost.** Nothing
is redacted for looking bad; a failed moonshot that teaches us where the
wall is is worth exactly as much here as an accepted one.

### L-000 — Founding decision: pay for ideas that fail (2026-10-15)

- **Proposal:** the Wildcard Track itself — pay for ideas the normal
  ladder structurally filters out, splitting the payout so the *published
  learning* earns its half even when the idea dies.
- **Verdict:** adopted by the owner as Track 3 of the bounty program.
- **What we learned (why this shape):** a feature bounty pays for merging,
  so it selects for incremental improvements by construction — anything
  that breaks the thing being improved cannot win it. Paying for the
  analysis instead of the merge inverts that filter: a paradigm-scale idea
  only has to be *falsifiable*, not mergeable, to earn. The repo already
  runs this pattern internally — the spike protocol pre-registers
  hypotheses and commits results *before* knowing them, and several
  "failed" spikes are exactly what de-risked the architecture that shipped.
  The Wildcard Track points that machinery outward.
- **Cost:** $500 per cycle + the review bandwidth to publish honestly.
  Deliberately accepted as the price of not being blind-sided by the idea
  we would have rejected.

---

## Failure wall

| ID | Idea | Why it died | What we learned |
|----|------|-------------|-----------------|
| — | *empty — kept here so it stays that way in public* | — | — |

*(Declined entries appear here with the published reason once the ledger
has them. A decline is not a judgment that the idea is bad — it is a record
of why we are not spending review bandwidth on it now, written so a
contributor can overturn it with one good argument.)*

---

## Active solicitation — what we want attacked this cycle

We actively solicit wildcard proposals; each cycle names the assumptions we
most want contradicted. Current prompts (answered via the wildcard
template):

1. **Kill the `.hy` / `.go` split.** Whole-module checking, or checking
   that attaches to ordinary Go via annotations — what would it take, and
   what guarantee dies with it?
2. **Replace the borrow-checker machinery.** Capability tokens,
   linear-type encodings inside Go's type system, runtime-enforced
   ownership — anything that deletes the alias/loan/freeze analysis rather
   than extending it.
3. **Attack the emit target.** Plain Go source is the current paradigm.
   Propose and analyze: emit to Go *IR*, a hybrid Go+Go-asm target, or
   deleting the Rust staticlib boundary entirely.
4. **"Impossible" guarantees.** Deadlock-freedom, async-cancellation
   safety, heap-freedom proofs for the checked subset — pick a guarantee
   GUARANTEES.md explicitly does not make and argue it is reachable.
5. **Rewrite us.** If the whole compiler should be a Go program
   (self-hosted analysis), a WASM engine, or a library rather than a CLI —
   say why and what it unlocks.

The standing invitation: **if you believe something in SPEC.md or
GUARANTEES.md is wrong, the Wildcard Track is the sanctioned place to say
so loudly.**

---

## Honest limits of this ledger

- Triage within 48 hours and the monthly windows are the AI maintainer's
  commitment; the maintainer keeps the final say on accept/decline/tier,
  every decision gets a written reason, and the owner retains budget
  authority and final appeal — the same governance as the ladder.
- Acceptance (M1) means "worth analyzing in public", not "will ship".
  Shipping decisions stay with the roadmap (PLAN.md) and its review gates.
- Learnings are published as written arguments, not verified measurements,
  unless an entry says a spike/job backed it — measurement-class claims
  still go through the spike protocol (pre-registered, committed results).
