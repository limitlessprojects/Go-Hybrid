# VERDICTS.md — the pre-registered verdict table (tests/checker-corpus)

**Status:** pre-registered 2026-10-12, BEFORE any checker existed — per the
human owner's ruling of 2026-10-12 (QUESTIONS.md Q1, item 2): *"write the
adversarial programs before the checker lands, and treat them as the
spec."* No checker, compiler, or interpreter existed when these verdicts
were derived; none was consulted (none exists). When the Phase-1 checker is
built, it must agree with this table. **Any disagreement is a bug in the
checker OR in this corpus, and is resolved by a logged amendment appended
to this file — never silently, never by rewriting history.**

**Normative brief:** QUESTIONS.md Q1 ruling — sendability reading **(c)+(b)**
(strictest), items 1–6 binding; spike-6 `RESULTS.md` Tier-0 rule set with
the strictest reading of every ambiguity (A1–A8). Diagnostic UX is
pre-registered below (Q1 item 3: friction must be a teaching moment, not
"not sendable").

Each `.hy` file carries one machine-readable line
(`// EXPECTED-VERDICT: ACCEPT | ACCEPT-MANAGED | REJECT <rule-id>`);
this file is the human-normative record.

## Rule key (IDs cited below)

| ID | rule (strict reading applied) | diagnostic code (SPEC §9.6) |
|----|-------------------------------|------------------------------|
| A1c | sendability, safe-by-type: a value may cross a spawn/send site only if it is a channel / atomic / immutable, an owned `move`, or `managed` | `HYB-E3001` (spawn) / `HYB-E3002` (send) |
| A1b | freeze-after-send: after an owned-move send, the sender — and every alias — must not use the value again (use-after-move; reads AND writes, binding AND aliases) | `HYB-E3003` |
| A2 | handle-loan rule (spike-added): mutating-mode calls on a boundary handle conflict with live borrow-mode loans on the same handle | `HYB-E3101` |
| A3 | map-slot borrows `&m[k]`: legal, non-lexical (creation → last use), invalidated by any map insert/delete inside the live window | `HYB-E3102` |
| A4 | Go-heap reads are copies, not loans (positive rule; exercised by 09 / 15 / 17) | — (enabling rule) |
| A5 | Go-API escape perimeter: values escaping into foreign Go code are outside the checked subset under the scoped-claim ruling | — (scope rule; opt-in lint TBD) |
| A6 | struct fields never hold borrow-handles (owned / GC / ids / own-handles only) | `HYB-E3103` |
| A7 | handles passed to functions are borrow-by-default; own-transfer happens only at `return` and at consuming APIs | `HYB-E3003` (use-after-consume) |
| A8 | top-level `?` exit contract — NOT exercised by this corpus (noted so nobody mistakes absence for coverage) | — |
| LOAN-ESCAPE | "loans never escape functions" (Tier-0 Layer-2 text; unambiguous, but has no A-number — flagged OQ4) | `HYB-E3104` family (SPEC O2) |

The code column exists so checker output, SPEC-LANGUAGE.md rules, and this
table are joinable from day one (registry format `HYB-Exxxx`, decided
2026-10-12 per research/competitive-dx-study.md; append-only, never
renumbered).

## Verdict table

| # | program | verdict | rule violated | one-line rationale |
|---|---------|---------|---------------|--------------------|
| 21 | 21-a7-borrow-in-closure | REJECT | LOAN-ESCAPE | closure captures borrow-handle and escapes the defining frame |
| 01 | 01-shared-map-across-tasks | REJECT | A1c | plain Go map read+written by spawned tasks — not safe-by-type/move/managed; also a Go runtime fatal |
| 02 | 02-shared-map-managed | ACCEPT-MANAGED | — (A1c escape route 3) | 01 wrapped in `managed`; checker accepts + managed lint fires; runtime honesty note in file |
| 03 | 03-pool-shared-struct-unguarded | REJECT | A1c | `&stats` — live borrow of a mutable GC struct shared across tasks, unguarded: a real data race |
| 04 | 04-pool-shared-struct-mutex | REJECT (contested) | A1c | race-free idiomatic Go (mutex-guarded), but a mutex is not safe-by-type under the ruled reading; **contested — see OQ2** |
| 05 | 05-sender-uses-after-move-send | REJECT | A1b | the `move` send is legal; the sender's later use is use-after-move (freeze violated) |
| 06 | 06-borrow-returned-from-fn | REJECT | LOAN-ESCAPE | `return &reg.counters[i]` — a slot loan would outlive the callee frame; loans never escape functions |
| 07 | 07-a2-mutate-during-handle-loan | REJECT | A2 | `doc.set(...)` (mutating-mode) while element-view borrow loans on `doc` are live |
| 08 | 08-a3-map-slot-invalidated | REJECT | A3 | map INSERT inside the live `&m[k]` slot-loan window (rehash invalidation) |
| 09 | 09-a3-map-slot-valid | ACCEPT | — | 08 with the insert moved after the loan's last use: A3's non-lexical window respected; A4 in passing |
| 10 | 10-a5-http-handler-capture | ACCEPT (contested) | — (A5 scoped claim) | mutable value captured by a closure escaping into net/http — no Hybrid check site exists; **contested — see OQ1** |
| 11 | 11-a6-borrow-in-struct-field | REJECT | A6 | borrow-handle (`serde_json.view`) stored in a struct field — the #1 annotation-forcing shape, forbidden by rule |
| 12 | 12-a7-borrow-by-default | ACCEPT | — | handle passed to two read calls and mutated afterwards: pins A7 borrow-by-default + A2 non-lexical windows |
| 13 | 13-a7-own-transfer-at-return | ACCEPT | — | own handle returned out of a function; caller mutates the returned handle: the A7 return-transfer route |
| 14 | 14-a7-use-after-consuming | REJECT | A7 | `enc.write(...)` after the consuming `enc.finish()` — use-after-own-transfer |
| 15 | 15-fanin-per-task-state | ACCEPT | — | per-task state + owned-move sends + single-task merge: the zero-`managed` redesign of 01 |
| 16 | 16-move-send-clean | ACCEPT | — | producer loop move-sends fresh batches and never touches them: the clean twin of 05 |
| 17 | 17-atomic-channel-immutable | ACCEPT | — | sharing via atomic + channel + const-immutable table — the safe-by-type legal story (contingent on OQ5/OQ6) |
| 18 | 18-pool-shared-struct-managed | ACCEPT-MANAGED | — (A1c escape route 3) | 04 wrapped in `managed`, lock kept: the runtime-honest escape shape |
| 19 | 19-freeze-read-through-moved-binding | REJECT (contested) | A1b (read-form) | pure READ through the moved binding after the send — pins SPEC O5.2 / open decision D1 (Q1's literal wording bans "mutations" only); **contested — flagged for G1 human pre-review** |
| 20 | 20-freeze-alias-write | REJECT | A1b (alias-form) | write through a live alias (`m2 := agg` before the send) — the freeze-defeat shape; alias coverage entailed by any sound freeze (05's REQUIRED MUTANT, materialized) |

**Distribution: ACCEPT 7 · ACCEPT-MANAGED 2 · REJECT 11 — 20 programs.**
(19 and 20 were added by the lead during integration on 2026-10-12 — still
BEFORE any checker existed; see the pre-registration statement.)

## Reading-distinguishing power (what Spike 6's corpus could not do)

Spike 6's ten programs passed under EVERY reading of A1, so they could not
settle anything. This corpus can:

- **05 distinguishes (a) from (b):** under (a) the sender may keep using a
  value after sending it; under (b) it may not. REJECT pins (b).
- **01 / 03 / 04 distinguish (c) from (a) and (b):** the shared mutable
  map/struct is sendable under (a); under (b)-alone it is ALSO sendable
  (the sender — main — never mutates after spawn, only the workers do, and
  (b) constrains only the sender). Only (c) rejects the spawn. REJECT pins (c).
- The loan rules (A2 / A3 / A6 / LOAN-ESCAPE / A7-consume) reject under
  EVERY A1 reading — they are reading-independent discipline rules.

So the corpus exercises the full (c)+(b) ruling, not just one half of it.

## Diagnostic message sketches (pre-registered — Q1 item 3)

Q1 ruling, item 3: *"not sendable" is hostile; "this map is shared across
tasks; wrap it in `managed`, or send ownership with `move`" is a teaching
moment.* The checker's diagnostic UX is therefore specced here, before the
checker exists. Shape requirements for EVERY reject: (1) name the value and
its type; (2) point at the exact site (site marker + expression); (3) list
the legal routes with cross-references; (4) for contested verdicts, say so
in the message — the user is told the rule is under amendment discussion,
not left guessing. Sketches use the corpus's site markers, not line numbers
(lines drift; markers are pinned in the headers).

### 01 — REJECT A1c (the ruling's own example)

```
error[A1c]: `counts` (map[string]int) is not sendable across tasks
  --> 01-shared-map-across-tasks.hy, spawn site [S1]
      (`task tally(move chunk, counts, done)`)
  this map is shared across tasks: mutable shared state must cross a
  boundary the checker can see. pick one:
    1. wrap it in `managed` (linted, counted by --report-managed)
       -> 02-shared-map-managed.hy is this program with that one change
    2. send ownership with `move`, if this task is done with it
       -> 05-sender-uses-after-move-send.hy shows the rule's edge
    3. keep the map per-task and merge over a channel (zero managed)
       -> 15-fanin-per-task-state.hy is the idiomatic rewrite
  note: as written this also panics in Go — concurrent map writes are a
  runtime fatal. the checker just tells you earlier.
```

### 03 — REJECT A1c (pointer shape)

```
error[A1c]: `&stats` (*pool_stats) is not sendable across tasks
  --> 03-pool-shared-struct-unguarded.hy, spawn site [S1]
      (`task worker(i, jobs, &stats, done)`)
  `stats` is a mutable struct shared as a live borrow — and these
  increments (`stats.processed++`) are unsynchronized: this is a data
  race today. pick one:
    1. per-worker stats, merged over a channel (zero managed)
       -> 15-fanin-per-task-state.hy
    2. wrap it in `managed` — and keep/add a lock, or the race remains
       -> 18-pool-shared-struct-managed.hy (managed + sync.mutex)
    3. `move` the struct into ONE task if only one task needs it
  note: guarded shape? see 04 — under Tier 0 as ruled, the mutex does not
  make the spawn sendable either (its diagnostic says so explicitly).
```

### 04 — REJECT A1c, CONTESTED (the load-bearing tripwire diagnostic)

```
error[A1c]: `&stats` (*pool_stats) is not sendable across tasks
  --> 04-pool-shared-struct-mutex.hy, spawn site [S1]
      (`task worker(i, jobs, &stats, done)`)
  this struct is shared across tasks and guarded by sync.mutex — the code
  is race-free by Go's rules, but Tier 0 does not credit locks at spawn
  sites (QUESTIONS.md Q1, reading (c)). pick one:
    1. per-worker stats, merged over a channel (zero managed, no hatch)
       -> 15-fanin-per-task-state.hy
    2. wrap it in `managed` (keep the lock; linted + counted)
       -> 18-pool-shared-struct-managed.hy
    3. send ownership with `move` if one task can own the stats
  note: an "externally synchronized" amendment (checker-verified exclusive
  access behind a lock) is under discussion for a later tier — this
  rejection is expected to be revisited; it is tracked, not silent.
```

### 05 — REJECT A1b

```
error[HYB-E3003]: `agg` is used after ownership was moved away
  --> 05-sender-uses-after-move-send.hy (`agg["total"] = len(agg)`)
  ownership of `agg` left this task at the move-send
      (`results <- move agg`, send site [S2]).
  after a move the sender — and every alias — must not use the value
  again. fixes:
    1. compute `total` BEFORE the send and send it along
    2. keep ownership here and send a copy
    3. if you meant to share it, that is a channel/managed question, not
       a move (see the A1c routes)
  note: over GC memory this cannot dangle — the rule is the single-
  logical-owner discipline, not memory safety.
```

### 19 — REJECT A1b, CONTESTED (D1: the reads-too question, made visible)

```
error[HYB-E3003]: frozen alias is read after the value was moved away
  --> 19-freeze-read-through-moved-binding.hy (`n := len(agg)`)
  ownership of `agg` left this task at the move-send
      (`results <- move agg`, send site [S2]), and the receiving task
  mutates it (`m["fox"]++` in sink). a read here races with that write
  (Go DRF-SC: concurrent unsynchronized read/write is a race). fixes:
    1. snapshot what you need before the send: n := len(agg) goes ABOVE
       `results <- move agg`
    2. have the receiver report stats back over `done`/a results channel
  note: the strict form (reads banned, not just writes) is provisional —
  open decision D1, pre-registered for human review at G1. if it relaxes
  to writes-only, the race-freedom claim narrows in the SAME edit; it
  will not change silently.
```

### 20 — REJECT A1b (alias-form)

```
error[HYB-E3003]: write through a live alias of a moved value
  --> 20-freeze-alias-write.hy (`m2["total"] = len(m2)`)
  `m2` aliases `agg` (bound at `m2 := agg`), and `agg` was moved at the
  send site [S2] (`results <- move agg`). the freeze covers every live
  alias — otherwise `m2 := agg` would defeat it entirely. fixes:
    1. drop the alias: if the sender needs the map after the send, do not
       move it — send a copy, or keep it and share via channel/managed
    2. compute `total` before the send and send it along
  note: this is a WRITE through an alias — rejected under every reading
  of the freeze rule, including the strictest-literal one. (The READ
  form's contested status lives in 19.)
```

### 06 — REJECT LOAN-ESCAPE

```
error[LOAN-ESCAPE]: a borrow cannot be returned from a function
  --> 06-borrow-returned-from-fn.hy (`return &reg.counters[i]`)
  `&reg.counters[i]` is a slot loan whose window ends when this function
  exits — Tier 0 keeps every loan function-local (that is why there are
  no lifetime annotations to write). fixes, in preference order:
    1. make the mutation a method on the owner, so the loan stays in one
       frame:  fn (r *registry) bump(i int) { ... &r.counters[i] ... }
    2. return the index and let the caller do the access
    3. return a copy (Go-heap reads are copies)
  note: returning &slice[i] is legal Go — this is a Tier 0 discipline
  rule, not a memory-safety fix. a lifetime parameter is NOT offered on
  purpose: the language has none.
```

### 07 — REJECT A2

```
error[A2]: mutating call on `doc` while a borrow loan is live
  --> 07-a2-mutate-during-handle-loan.hy (`doc.set("pruned", true)`)
  the iteration `for u in doc.array("users")?` holds borrow-mode loans on
  `doc` for the whole loop; `doc.set(...)` is a mutating-mode call inside
  that window. fix: collect-then-mutate — reads at the boundary are
  copies, so gather what you need into locals, let the loan die at the
  loop's last use, THEN mutate (see 12-a7-borrow-by-default.hy for the
  legal after-the-loop shape).
```

### 08 — REJECT A3

```
error[A3]: map insert invalidates the live slot loan `svc`
  --> 08-a3-map-slot-invalidated.hy (`e.services[fresh] = ...`)
  `svc := &e.services[existing]` is live from its creation to its last
  use (`svc.tag = "scaled"`), and this INSERT falls inside that window —
  a rehash may move the slot. fixes:
    1. reorder: finish the slot mutation first (one line moves — see
       09-a3-map-slot-valid.hy, the legal twin)
    2. copy-out / modify / store-back (reads are copies)
```

### 11 — REJECT A6

```
error[A6]: struct field `auditor.cur` has borrow-handle type
       `serde_json.view`
  --> 11-a6-borrow-in-struct-field.hy (field declaration + `a.cur = u`)
  fields hold owned values, GC values, ids, or own-handles — never
  borrows: a stored borrow is the shape that would force lifetime
  annotations. fixes:
    1. process the element inside the loop body (the loan dies at each
       iteration's last use)
    2. store the index or a copy, and re-view through the handle later
```

### 14 — REJECT A7

```
error[A7]: `enc` is used after ownership was consumed
  --> 14-a7-use-after-consuming.hy (`enc.write([]byte("trailer"))`)
  ownership of `enc` was transferred by the consuming call
      (`enc.finish()`, marked `consume` in the boundary descriptor).
  fixes:
    1. write the trailer BEFORE finish()
    2. create a new writer for the trailer
  note: the transfer site is reported above so you can see where
  ownership left — not just the line that noticed.
```

### ACCEPT-MANAGED lint sketches (Q1 item 4 — the hatch, named honestly)

```
02: lint[managed]: 1 region, 4 permitted spawn sites
      (map[string]int shared across tasks) -- counted by --report-managed
    note (from the corpus): managed suppresses the sendability check; it
    does not remove Go's concurrent-map-write runtime fatal — prefer the
    channel redesign (15) or add a lock (18's shape).

18: lint[managed]: 1 region, 4 permitted spawn sites
      (*pool_stats shared across tasks; guarded by sync.mutex -- Go's
       DRF-SC story applies inside the hatch) -- counted by --report-managed
```

The lint text carries WHAT is shared and WHY it is believed safe — an
auditor reading `--report-managed` output sees the trust surface, not just
a count (Q1 item 4: greppable, auditable, countable).

## Tripwire baseline (Q1 item 5)

Two numbers, with the definitions fixed now so the checker-era recompute
compares like with like:

1. **Verdict-level managed:** programs whose registered verdict is
   ACCEPT-MANAGED — **2 / 20 = 10.0%** (02, 18).
2. **As-written managed (the friction number):** programs whose
   idiomatic-Go shape cannot survive AS WRITTEN without `managed` or a
   redesign — **3 / 20 = 15.0%** (01, 03, 04). 04 counts only while
   lock-mediated sharing stays outside Tier 0 (OQ2); if that amendment
   lands, this drops to 2 / 20 = 10.0%.

**Both numbers are below the ~25% tripwire → G0 stays closed at baseline.**
Honest caveats: the corpus is small (20) and deliberately loaded toward
the reject side, so these are a baseline, not a market estimate; the other
eight REJECTs (05, 06, 07, 08, 11, 14, 19, 20) are
genuine bugs or loan-discipline violations that any defensible ownership
model rejects — they are the checker working, not friction. When the
checker runs, recompute both numbers on these exact definitions; reopening
G0 requires >~25% on the as-written definition (the lead's concretization
of the human's "more than ~X%"; the number is recorded as tunable).

## Pre-registration statement

Every verdict above was derived BY HAND from the A1–A8 strict readings and
the (c)+(b) ruling in QUESTIONS.md, BEFORE any checker existed, by agent
7-b (checker-corpus). Programs 01–18 are 7-b's pre-registration; **19 and
20 were added by the LEAD during integration on 2026-10-12, still before
any checker existed**, materializing the REQUIRED MUTANT recorded in 05's
header (OQ3) and SPEC-LANGUAGE.md's open decision D1 — both were flagged
gaps, not new semantics. Verdicts may change ONLY by a logged amendment
appended below (date, program, old → new verdict, reason, author) — never
by editing the table above, never silently. A checker/checker-corpus
disagreement is resolved by deciding which side is wrong and amending the
artifact that is wrong, with the decision recorded.

**Amendment log:** (empty — no amendments since pre-registration
2026-10-12)

## Open questions (flagged for SPEC-LANGUAGE.md — NOT silently decided)

These could not be settled from the ruling + spike-6 text; each corpus
program that depends on one says so in its header. None was decided by
fiat here.

- **OQ1 (A5, fourth check-site class).** Values captured by closures
  passed to concurrent Go APIs (net/http handlers) escape all Tier-0 check
  sites. Decided HERE only as the scoped-claim default (10 = ACCEPT,
  contested); if SPEC-LANGUAGE adds a "concurrent-flagged Go API" check
  site, 10 flips to REJECT. This is the corpus's most user-visible open
  decision: request-counter middleware is everywhere.
- **OQ2 (A1c amendment: lock-mediated sharing).** 04 is race-free idiomatic
  Go rejected under (c). An "externally synchronized" clause
  (checker-verified exclusive access behind sync.mutex) is the strongest
  Tier-1 amendment candidate. NOT Tier 0 as ruled; the strict verdict
  stands until an amendment is logged.
- **OQ3 (A1b scope).** ~~The freeze must cover reads~~ **RESOLVED by
  materialization 2026-10-12 (lead):** 19 pins the read-form (contested —
  D1, flagged for G1 human pre-review; SPEC O5.2 bans reads and this
  corpus agrees) and 20 pins the alias-write-form (uncontested). The
  checker must reject both; a logged amendment is required to flip either.
- **OQ4 (loan-escape numbering).** "Loans never escape functions" is
  Tier-0 text with no A-number (06 cites LOAN-ESCAPE). It is A6's sibling
  (field escape vs return escape — same principle, different site); SPEC-
  LANGUAGE.md should number them together so the checker has one rule
  family, not two.
- **OQ5 (const composites).** 17 shares a `const` array across tasks and
  calls it immutable-by-construction. SPEC-LANGUAGE must pin: do const
  composites exist, and do views/slices of them stay immutable for A1c?
- **OQ6 (pointer-to-atomic).** 17 sends `&hits` where `hits` is
  atomic-typed; the checked property is the pointee's type. SPEC-LANGUAGE
  must state that borrows of atomic-typed values are sendable (the
  intended mediated route — Go code needs the pointer form).
- **OQ7 (mutating a borrowed-in handle).** Can a callee make MUTATING-mode
  calls on a handle passed borrow-by-default? Spike-6 05's header treated
  the borrow-mode parameter as read-only use; the corpus deliberately
  avoids the shape (12/13 dodge it). Needs a decision + then a corpus
  program; not decided here.
- **OQ8 (consuming-API surface).** 14 assumes `consume` is flagged in the
  boundary descriptor (interfaces/abi-types.md) for Rust-side APIs. Open:
  can Hybrid-side (non-boundary) methods be consuming, and where is the
  flag visible to users?
- **A8 note.** The top-level `?` exit contract is not exercised by this
  corpus (no program fires it) — flagged so SPEC-LANGUAGE coverage is not
  overstated; spike-6 02 remains its only test.
