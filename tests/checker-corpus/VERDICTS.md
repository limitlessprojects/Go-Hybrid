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
| IMMUT | T22 deep immutability: a `const` binding is frozen — no rebind, no element/index write, no field store through it; reads stay legal (the file-level form lowers to package-level state every task body reaches ambiently) | `HYB-E3006` |

The code column exists so checker output, SPEC-LANGUAGE.md rules, and this
table are joinable from day one (registry format `HYB-Exxxx`, decided
2026-10-12 per research/competitive-dx-study.md; append-only, never
renumbered).

## Verdict table

| # | program | verdict | rule violated | one-line rationale |
|---|---------|---------|---------------|--------------------|
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
| 21 | 21-let-shared-map-reject | REJECT | A1c | `let`-bound map crossing a spawn site — T22 says `let` is SHALLOW, so it is a named mutable GC value, not class-4 immutable (regression for issue #12; corpus growth, added after the fix) |
| 22 | 22-a1c-mapslot-loan-at-spawn | REJECT | A1c | a LIVE map-slot loan (`&e.services[k]`, O3 form 3/O8) crosses a `task` spawn site as-is — A1c's safe-by-type test runs on the loan's pointer type exactly as it would for a plain struct borrow; the loan's special provenance grants no exemption |
| 23 | 23-a1c-mapslot-copy-at-spawn | ACCEPT | — | 22's legal twin: the map-slot field is read into a local BEFORE the spawn (the loan's last use), so a copy (A4/O9) crosses the spawn site instead of the live loan — pins that A1c tracks the LOAN crossing, not the map-slot's mere upstream existence |
| 24 | 24-shadow-scope-map-reject | REJECT | A1c | an inner-block `m := 1` must SHADOW the outer map, never overwrite its binding — the flat env let the inner define rewrite the outer shape and the spawn passed with zero diagnostics while `go -race` reported two write/write DATA RACEs (issue #14; the env is now scope-aware) |
| 25 | 25-o40-spawn-callee-proof | REJECT | A1c | the O4.0 spawn-callee PROOF: a closure callee (`task gap()`) crosses with its captured environment (A5, E3001) and an undeclared callee (`task mystery(1)`) is fail-closed unproven (E3005/FC) — before the fix the O4.0 site checked only the arguments and both passed silently (adoption-audit finding F1; the ACCEPT/proven side is pinned in hy-check/tests/callee_and_nil.rs) |
| 26 | 26-call-return-alias-freeze | REJECT | A1b | an alias that comes back through a function call (`alias := same(agg)`, `same` returns its map argument) is still a live alias of `agg` — the checker has no interprocedural summary proving the return is fresh, so `results <- move agg` freezes it too; the write through `alias` is use-after-move (issue #38; before the fix the call return was treated as fresh storage and the program was accepted with 0 diagnostics while `go -race` reported a DATA RACE) |
| 27 | 27-const-write-reject | REJECT | IMMUT | a file-level `const` slice takes an element write (W1) and a whole-binding rebind (W2) while the emitter lowers the const to a package-level `var` — two spawned tasks reach the same backing array with NOTHING crossing the spawn site, so O4 never fires and `hyb check` reported 0 diagnostics against `go run -race` evidence (issue #37; T22 deep immutability — the write rejection was the missing check; the accept side is pinned in hy-check/tests/const_write.rs) |
| 38 | 38-const-closure-alias-write-reject | REJECT | IMMUT | T22 deep immutability on the paths the first E3006 fix never walked: an element write, a rebind, and a `delete` INSIDE a closure body (flatten/uses/calls skip `FnLit` — the A5 perimeter), a write through a COPY of the const (`alias := cells` — `const_write_root` only tested the place's own root), and `delete` on a const map anywhere — all ACCEPT with 0 diagnostics on main while `go run -race` reported `WARNING: DATA RACE` for the dual-closure writer program (issue #123, the follow-up gap in paid #37's fix; the fix scans every FnLit at fn end, follows alias/may-share peers to the immutable root, and routes delete through E3006 — accept side pinned in hy-check/tests/const_write.rs) |



| 28 | 28-composite-nil-reject | REJECT | N1 | a nil NESTED in a composite literal (`job{m: nil}`, map-typed field) passed with 0 diagnostics — classify_composite has no per-field type context, so the five direct-flow nil sites never saw it; the literal carries its own type context and the nil now rejects as a TYPE error at the define, at the boundary, inside `managed` too, and for map/slice literal slots (PLAN follow-up row from the external adoption audit) |
| 29 | 29-struct-param-share-move-reject | REJECT | A1b | a shallow struct copy of a PARAMETER (`h2 := h`, struct carrying a map) is a may-share peer but not a reference alias, so `check_non_owned_move` (which consulted the binding's own origin and its alias-chain root only) accepted `task worker(move h2, done)` with 0 diagnostics while `go run` hit `fatal error: concurrent map writes` (issue #75; the map twin `m2 := m` was already rejected via alias+origin inheritance — identical storage-identity argument; array twin is corpus 30, pure-local accept pinned in hy-check/tests/param_share_move.rs) |
| 30 | 30-array-param-share-move-reject | REJECT | A1b | ARRAY twin of 29: a shallow fixed-array copy of a parameter (`a2 := a`, `[2]map[string]int`) recurses to a map element shape so it may-shares, but a fixed array is not a reference alias and recorded no origin — same pre-fix OK(0)/`go run` fatal split, same fix (move-time scan of the may-share component for a non-owned origin) |
| 31 | 31-generic-call-return-alias-freeze | REJECT | A1b | an alias that comes back through a GENERIC call (`alias := same(agg)` where `same[t any](m t) t` returns its argument) is still a live alias of `agg` — the result shape resolves through the type parameter to Unknown, which is not a reference alias, so 26's alias recording and the may-share recorder (which had no Call arm) both skipped it: before the fix `hyb check` reported 0 diagnostics while `go run` hit `fatal error: concurrent map writes` (issue #73; generic twin of 26 — the struct-result sibling `h2 := alias_holder(h1)` is pinned in the hy-check unit tests, one root cause) |
| 32 | 32-a3-assign-map-slot-invalidated | REJECT | A3 | re-assignment `x = &m[k]` opens a map-slot loan window just like define `x := &m[k]` — before the fix only LocalDecl registered MapSlot loans, leaving assignments untracked so map inserts during active assignment loans passed with 0 diagnostics (issue #138) |


| 32 | 32-closure-local-alias-move-reject | REJECT | A1b | a local rebind inside a closure (`alias := m`, where `m` is a captured outer map) is an alias of non-owned state, but `check_closure_boundaries` called `define_binding` without `record_alias` or `record_may_share`, wiping origin and alias links — before the fix `task worker(move alias, done)` was accepted with 0 diagnostics while `go run -race` produced concurrent map writes (issue #137) |


**Distribution: ACCEPT 10 · ACCEPT-MANAGED 2 · REJECT 24 — 36 programs.**
| 36 | 36-closure-capture-move | REJECT | A1b | a closure that captures a reference-bearing outer binding is a live ALIAS of its storage: `ch <- move a` transfers the map, but a later `f()` writes through the capture — on main the program was ACCEPT with 0 diagnostics while the receiver and the closure wrote the same map (`go run -race` WARNING: DATA RACE, issue #109); the fix links the closure binding to its captures with may-share edges so the move freezes `f` (alias-read twin of 20's alias-write form; accept/precision side pinned in hy-check/tests/capture_freeze.rs) |
| 37 | 37-a3-interproc-window | REJECT | A3 | the insert that invalidates the live `&m["a"]` slot loan is spelled one FRAME away — `bump(m, "fresh")` inserts in its own frame while main's non-lexical window is live — the same runtime event as 08's direct insert (which rejects HYB-E3102); on main this form was ACCEPT with 0 diagnostics while the direct twin rejected (issue #111, with alias and closure-body siblings plus a wrong-result runtime proof: a helper delete resurrected by the loan's store-back); the fix matches A3 events across share-group peers, callee parameters (bounded, fail-closed), and closure call sites (legal twins pinned in hy-check/tests/a3_interproc.rs) |

**Distribution: ACCEPT 10 · ACCEPT-MANAGED 2 · REJECT 26 — 38 programs.**


**Distribution: ACCEPT 8 · ACCEPT-MANAGED 2 · REJECT 27 — 39 programs.**
| 40 | 40-wrapper-alias-freeze | REJECT | A1b | the WRAPPER tail of the same family: `alias := same(same(agg))` — and the sibling forms `same(agg)[0:2]`, `wrap{items: same(agg)}`, `box{items: agg}.items`, `take(box{items: agg})` — is still a live alias of `agg`, but `Expr::root_ident()` stops at a call or a literal base, so neither `record_alias` nor `record_may_share` recorded an edge: before the fix `hyb check` reported 0 diagnostics while `go run -race` reported `WARNING: DATA RACE` on the shared map write (issue #160; nested twin of 26/31, fixed by generalizing the alias collector) |
| 41 | 41-wrapper-alias-own-move-control | ACCEPT | — | the FALSE-POSITIVE GUARD for 32: the same wrapper alias is held and read BEFORE the owned move, and neither `agg` nor `alias` is used after it — holding an alias is legal, only a use after the freeze violates O5.2, so a widened alias collector must keep this program accepted (accept-side control; pairs with 32 the way 23 pairs with 22) |

**Distribution: ACCEPT 11 · ACCEPT-MANAGED 2 · REJECT 27 — 41 programs.**


**Distribution: ACCEPT 12 · ACCEPT-MANAGED 2 · REJECT 28 — 42 programs.**

## Additive corpus registration — 2026-10-15: builtin `copy` ref-element alias (issue #178)

These two cases were hand-derived from SPEC O5.2 (freeze covers every live
alias) and Go's element-copy semantics for the builtin `copy` before being
run against the checker. For a slice whose element carries mutable GC
storage, `copy(dst, src)` duplicates the element *references*, so `dst` and
`src` become may-share peers; the checker recorded no edge (there is no
`copy` arm anywhere in `hy-check`) and accepted the REJECT case with 0
diagnostics while `go run -race` reported `WARNING: DATA RACE` and `fatal
error: concurrent map writes`. The control guards the false-positive
direction: a primitive-element slice copies by value and shares nothing.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 43 | 43-copy-ref-element-alias-freeze | REJECT | A1b | `copy(dst, src)` on a `[]map[...]` makes `dst[i]` and `src[i]` the same map; `task worker(move dst, done)` did not freeze `src`, so the later write through `src` was use-after-move (issue #178; the root cause is a missing `copy` arm in `process_call`, plus `make([]T, n)` dropping its element shape) |
| 44 | 44-copy-primitive-element-control | ACCEPT | — | the FALSE-POSITIVE GUARD for 43: `copy(dst, src)` on `[]int` copies by value and shares no GC storage, so no may-share edge may be recorded |
| 45 | 45-composite-literal-embeds-shared-map | REJECT | A1c | O4.6 deep sendability: a fresh `[]T{shared}` composite literal embedding a live shared map at a spawn is not laundered by the wrap (PR #176, maintainer-integrated as #45) |
| 53 | 53-array-slice-backing-freeze | REJECT | A1b | slicing an addressable array (`s := a[:]` on `[4]int`, also `b.data[:]`, `append(a[0:1], x)`, overlapping windows) shares the array's backing storage even when the element is a primitive; `shape_may_share` only recurses into reference elements, so `move s` froze `s` and not `a`, `hyb check` reported 0 diagnostics, and `go run -race` reported `WARNING: DATA RACE` on `s[0]` vs `a[0]`. A value copy (`b := a`) and `clone(a)[:]` stay unlinked |

**Distribution: ACCEPT 13 · ACCEPT-MANAGED 2 · REJECT 34 — 49 programs.**

(19 and 20 were added by the lead during integration on 2026-10-12 — still
BEFORE any checker existed; see the pre-registration statement. 21, 22, 23
and 24 are post-checker corpus GROWTH from the bounty track, added
2026-10-14: 21 fixes issue #12 (PR #13), 22 and 23 close bounty issue #6
(PR #15, renumbered from its 21/22 spelling during the merge seam), and 24
fixes issue #14 (the lead's fix; reported by LubuSeb with a negative
control) — each verdict was still hand-derived from the A1–A8 strict
readings first, per their own headers, and only then reproduced against
the checker per BOUNTY.md's "pay only after reproduction". 25 is the
adoption-audit batch (2026-10-14): the O4.0 spawn-callee proof + the N1
nil rule emission, from the external audit's findings F1/F2 — verdict
derived from O4.0/A5/FC first, then pinned against the fix.) 26 is the
issue-#37 soundness program (2026-10-14): verdict derived from T22's
deep-immutability text first, then pinned against the HYB-E3006 fix —
the report came with `hyb check` OK / `go run -race` DATA RACE evidence,
so the hand-derived REJECT and the reproduction were one step here, in
that order. 26 is the issue-#38 call-return alias (2026-10-14, renumbered
when pr-42 took the slot) and 27 is the issue-#37 soundness program as the
table above spells them. 28 is the issue-#109 closure-capture freeze
(2026-10-15): verdict derived from O5.2's alias form first (a capture is a
live alias of the captured storage), then pinned against the fix — the
report came with `hyb check` ACCEPT / `go run -race` DATA RACE evidence,
so the hand-derived REJECT and the reproduction were one step, in that
order. 29 is the issue-#111 interprocedural A3 program (2026-10-15):
verdict derived from O8's own text first ("any map insert/delete inside
the window" — no frame scoping) and pinned against the fix; the report
came with the direct-twin contrast (corpus 08 / direct delete reject,
these accept) and a runtime wrong-result reproduction, hand-derived
REJECT first, per the same discipline.

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
   ACCEPT-MANAGED — **2 / 22 = 9.1%** (02, 18).
2. **As-written managed (the friction number):** programs whose
   idiomatic-Go shape cannot survive AS WRITTEN without `managed` or a
   redesign — **3 / 22 = 13.6%** (01, 03, 04). 04 counts only while
   lock-mediated sharing stays outside Tier 0 (OQ2); if that amendment
   lands, this drops to 2 / 22 = 9.1%. 21 and 22 add a REJECT/ACCEPT pair
   on the A1c axis that was already covered (not a new friction point):
   neither is counted in either numerator.

**Both numbers are below the ~25% tripwire → G0 stays closed at baseline.**
Honest caveats: the corpus is small (22) and deliberately loaded toward
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
gaps, not new semantics. **21 and 22 were added 2026-10-14 for bounty
issue #6, AFTER the Phase-1 checker landed**: each verdict was still
hand-derived from the rule text first (see their own headers), and the
checker run was a reproduction step per BOUNTY.md, not the source of the
verdict — the pre-registration discipline applies to them unchanged.
Verdicts may change ONLY by a logged amendment
appended below (date, program, old → new verdict, reason, author) — never
by editing the table above, never silently. A checker/checker-corpus
disagreement is resolved by deciding which side is wrong and amending the
artifact that is wrong, with the decision recorded.

**Amendment log:**

- **2026-10-14 — corpus GROWTH, `21-let-shared-map-reject` (REJECT A1c).**
  Not a verdict change to an existing program: a new regression program
  added with the fix for issue #12. The checker had been promoting a
  `let`-bound map (and `let`-rooted field/index reads) to class 4 via
  `LocalKind::is_immutable()` (Let | Const), so this program was ACCEPTED
  and the emitted Go raced. T22 (`let` is shallow) and the O4 class-4 row
  (`const` values) settle it: the `let` arm must not grant class 4. Fix:
  `process_local` grants the class-4 shortcut only to `LocalKind::Const`.
  Author: @Ratanlavate.

- **2026-10-14 — corpus GROWTH, `27-const-write-reject` (REJECT IMMUT).**
  Not a verdict change to an existing program: a new regression program
  added with the fix for soundness issue #37 (P0 — data race reachable in
  the checked subset). T22 settles the verdict by its text: `const` is the
  only DEEP immutability in MVP, so the element write `cells[0] = ...`
  and the rebind `cells = [5]` must both be compile errors — yet the
  checker had no write-rejection site at all for `const` roots (bindings
  were only protected against shape rewrites elsewhere), the emitter
  lowered the file-level const to a package-level `var`, and two spawned
  tasks raced with `hyb check` reporting 0 diagnostics (nothing crossed
  the spawn site, so O4 never fired — distinct from #25/#12). Fix: new
  `HYB-E3006` (rule T22, tier0 `IMMUT`) emitted from `process_assign` via
  a `const_write_root` walk (Ident/Field/Index to the binding root;
  locals-first lookup keeps shadowing `let` copies writable). Verdict
  hand-derived from T22 first; the report's `go run -race` evidence is
  the reproduction, not the source of the verdict.

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

## Additive corpus registration — 2026-10-14: select-send clauses

The original registrations and subsequent upstream cases 21–24 above are
preserved. The following three cases were derived from SPEC-LANGUAGE.md
§8.3 C4, §8.4 C5 and §9.3 O4/O4.9 before these files were run against the
checker. They exercise the previously unrepresented `select` SEND surface,
not a new language rule. Each file pins one verdict; reject files also
name the exact diagnostic code in their header. A disagreement must be
recorded and resolved, never hidden by changing an expected result.

Originally registered as 21–23, these files are now 25–27: the integration
of upstream commit `2e79213838649557cb00310a9f79c166f8c6731c` brought its own
cases 21–24. Only filenames and corresponding references were renumbered;
expected verdicts, rules and diagnostic codes did not change.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 32 | 32-select-send-shared-map | REJECT | A1c | `HYB-E3002` | a named mutable map is not safe-by-type at a select-send clause |
| 33 | 33-select-send-safe-values | ACCEPT | — | no error | string and channel values remain legal at select-send sites |
| 34 | 34-select-send-unproven-name | REJECT | FC (O4.9) | `HYB-E3005` | an unresolved sent name is unproven, even with a known destination element type |

Historical registration on the earlier 20-case base produced **23
programs: ACCEPT 8 · ACCEPT-MANAGED 2 · REJECT 13**; the recorded 23-case
verification remains a dated result rather than a claim about this merged
state. With the upstream 24-case base, this three-case addition gives **27
programs: ACCEPT 9 · ACCEPT-MANAGED 2 · REJECT 16** before case 28 below.
`FC` is the existing fail-closed rule from SPEC O4.9; these rows do not
amend its meaning or renumber diagnostics. No new file contains `managed`.
The harness's static as-written managed set remains the historical
{01, 03, 04}; its ratios count only that set, not a new exhaustive friction
survey. The dated original tripwire baseline above remains historical.

## Additive corpus registration — 2026-10-14: default fallback

Case 28 (originally registered as case 24 before upstream integration) was
derived from SPEC-LANGUAGE.md §8.3 C4 before running this fixture. It pins
the already documented default-arm syntax and selection behaviour, not a
new language rule. The checker must accept the safe string send. At
runtime the unbuffered channel has no receiver, so the default arm must
run without blocking and print exactly `idle`.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 35 | 35-default-fallback | ACCEPT | — | no error | a default arm supplies the nonblocking fallback for an unavailable, safe string send |

This is an ordinary syntax/behaviour regression: the previous parser
expected a reserved `default` token that the lexer does not emit. C4
requires the arm syntax; LX3 leaves `default` as an ordinary identifier.
Its previous parse rejection is not an accepted-but-unsound program.
The expected ACCEPT verdict and output were registered before testing and
must not be changed merely to match a checker result. Contextual parser
recognition restores documented arm syntax without reserving a new
keyword; cases 25–27 stand alone without defaults.

Historical registration on the earlier base produced **24 programs:
ACCEPT 9 · ACCEPT-MANAGED 2 · REJECT 13**. That registration and its local
verification are retained as dated evidence; they are not the merged
corpus total. Current registered corpus: **28 programs: ACCEPT 10 ·
ACCEPT-MANAGED 2 · REJECT 16**. The original and upstream table entries and
dated baseline remain unchanged. Verdict-level managed count is 2/28;
the harness's unchanged historical as-written set {01, 03, 04} reports
3/28, with the same population qualification noted above. Case 28 adds
no `managed` block.

## Additive corpus registration — 2026-10-15: buildability declarations (#134)

Four cases derived from issue #134 before these files were run against the
checker. They pin the accepted-subset contract that an accepted program
must emit buildable Go (the same contract E6001/E6002 were registered
under): Go's front-end rejects an unused `:=` binding, an unused import, a
repeated name on the left of one `:=`, and a `:=` that declares no new
variable at all. All four reject as the new code `HYB-E1002`
(`unbuildable-declaration`, rule `BUILDABLE`, registered in
`hy-diag/data/diagnostics.json`). Reject files name the exact diagnostic
code in their header; a disagreement must be recorded and resolved, never
hidden by changing an expected result.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 46 | 46-unused-define-reject | REJECT | BUILDABLE | `HYB-E1002` | `x := 5` is never read — the emitted Go is `declared and not used` |
| 47 | 47-unused-import-reject | REJECT | BUILDABLE | `HYB-E1002` | the declared import is carried into the emitted header — `imported and not used` |
| 48 | 48-repeated-define-name-reject | REJECT | BUILDABLE | `HYB-E1002` | `x, x := 1, 2` is `x repeated on left side of :=` |
| 49 | 49-blank-define-reject | REJECT | BUILDABLE | `HYB-E1002` | `_ := 5` declares no new variable — `no new variables on left side of :=` |

With this four-case addition the registered corpus is **49 programs:
ACCEPT 13 · ACCEPT-MANAGED 2 · REJECT 34** (harness recompute: 49/49
verdict agreement; tripwire 2/46 = 4.3% verdict-level, 3/46 = 6.5%
as-written — not fired). None of the four files contains a `managed`
block; the harness's historical as-written set {01, 03, 04} is unchanged,
so the tripwire ratios only move with the larger total.

The scope boundary is deliberate and recorded in the HYB-E1002 registry
entry: an unused `rust/` import is NOT rejected (the emitter never writes
it as a Go import, so it cannot fail `go build`); unused `var` /
composite-`const` locals, select-receive names and match bindings stay
outside this registration (the same Go error family, tracked separately);
unread `for ... in` bindings are handled at emission (issue #139).

## Additive corpus registration — 2026-10-17: 3-argument slice `make` (#353)

One case added WITH its fix: `make([]T, len, cap)` is valid Go, was
rejected as HYB-E2005 before the fix, and is accepted now. The 3-argument
form is slice-only — the same arity on a channel stays E2005, and
negative/bad-literal sizes still reject, pinned by unit tests in
`hy-check/tests/builtin_args_defer.rs`. The old diagnostic claimed "Go
rejects this builtin shape" — false for the slice form, and true for every
remaining rejection.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 51 | 51-make-slice-cap | ACCEPT | — | — | `make([]int, 3, 10)` is valid Go; zero check-sites, a fresh local |

With this addition the corpus on disk stood at **51 programs:
ACCEPT 13 · ACCEPT-MANAGED 2 · REJECT 36** (recomputed from the files'
`EXPECTED-VERDICT` markers; #50 carries its registration with its own fix
PR; the registration below raises the total to 52).

## Additive corpus registration — 2026-10-17: shadowed predeclared types (#326)

One case derived from issue #326 before running this fixture. It pins the
soundness rule that user-defined structs or type aliases shadowing predeclared
primitive names (e.g. `struct rune { m map[string]string }`) must resolve to the
user struct rather than falling through to primitive type shape. When passed
across tasks, non-sendable contents (such as maps or slices) must be rejected
with `HYB-E3001` (A1c sendability).

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 52 | 52-shadow-predeclared-struct-reject | REJECT | A1c | `HYB-E3001` | `struct rune { m map[string]string }` passed across tasks must reject because the shadowed struct contains a mutable map |

With this addition the registered corpus is **52 programs:
ACCEPT 13 · ACCEPT-MANAGED 2 · REJECT 37** (harness recompute: 52/52
verdict agreement; tripwire 2/52 = 3.8% verdict-level, 3/52 = 5.8%
as-written — not fired).

## Additive corpus registration — 2026-10-17: field store materializes may_share (#392)

`h.m = shared` never created the edge from the composite to the live RHS root,
so `move h` froze `h` but not `shared` and the emitted Go raced.

| # | program | verdict | rule | diagnostic | rationale |
|---|---------|---------|------|------------|-----------|
| 54 | 54-field-store-alias-at-spawn | REJECT | A1b | `HYB-E3003` | `var h holder; h.m = shared` then `task worker(move h)` then a write to `shared` |
| 55 | 55-field-store-fresh-at-spawn | ACCEPT | — | — | the over-rejection guard: a FRESH map literal stored into the field shares nothing |
| 56 | 56-callee-store-alias-at-spawn | REJECT | A1b | `HYB-E3003` | `set(dst, shared)` stores the map into `dst[0]` inside a declared helper, then `task worker(move shared)` then a write to `dst[0]` |
| 57 | 57-callee-store-fresh-control | ACCEPT | — | — | the over-rejection guard: a declared call that only reads two maps creates no may-share edge |
