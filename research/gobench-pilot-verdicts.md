# GoBench pilot verdicts — and a correction to the protocol

**Follow-up to** [#1890](https://github.com/limitlessprojects/Go-Hybrid/issues/1890), PR #2014. Pre-registered step: transliterate the 7 single-spawn kernels first, publish per-kernel verdicts, do not aggregate early.

## 0. Correction that changes the metric

The census in PR #2014 measures *site shape*, which is version-independent. The per-kernel verdicts are **not**: GoBench ships the **buggy** version of each kernel, and the whole point of the A1 rule is to reject buggy code. A rejection on a buggy kernel is a **true positive**, not evidence of cost.

So the O18 cost number must be computed on the upstream-**fixed** versions:

- buggy version rejected → the checker works (sanity check, not cost);
- fixed version rejected or forced into `managed` → that is the false-positive cost O18 is trying to price.

I am stating this now, before running the remaining kernels, because the first two verdicts below would otherwise be scored backwards.

## 1. Verdicts so far (3 of 7)

### moby/27037 — closure capturing the loop variable

Go source: `for i := 17; i <= 21; i++ { go func(){ _ = fmt.Sprintf("v1.%d", i) }() }` — the classic pre-Go-1.22 loop-variable capture race.

Minimal lift: hoist the body into `fn report(i int)` and spawn `task report(move i)`.

**Verdict: `OK (0 diagnostics)`.** Hybrid's `move` makes the capture explicit and per-iteration, so the bug is *unrepresentable*, not merely undetected. This is the strongest possible outcome for A1 and costs nothing: the site is provable after a mechanical lift.

### moby/22941 — closure capturing an interface value and a byte slice

Parser gap first: `interface Conn { Write(b []byte) }` is in `SPEC-LANGUAGE.md` but the parser rejects `interface` at top level (`HYB-E0002`). The transliteration therefore collapses `Conn` to the concrete `*pipe`, which is already lossy.

**Verdict on the concrete version: `HYB-E3001` — "`&p` (*pipe) is not sendable across tasks … mutable struct shared as a live borrow"**, plus `HYB-E3005`. For the buggy kernel this is correct: the shared writer is the race. Whether the *fixed* version survives depends on the upstream patch shape (mutex inside `pipe` still means a shared pointer crosses the task), so on current evidence this kernel's fixed form likely needs `managed("…","mutex")` or a channel rewrite.

### serving/5865 — method spawn `go rw.run()`

**Verdict: `HYB-E3005`** — "the checker can only prove spawns of declared top-level `fn`s — `rw.run` is a method value or a computed callee". The kernel is a one-line method spawn; there is no lift that preserves the method form.

## 2. What the 7-kernel set already tells us

| Site shape | Kernels | Expected verdict |
|---|---|---|
| closure | 5 | liftable in principle; depends on captures |
| method | 2 | **unprovable as written**, needs lift or `managed` |

So the floor is **2 of 7 forced into `managed`/restructure** (28.6%) — on its own above the ~25% O18 trigger. The closure cases can only lower the cost if their captures are movable values; `moby/27037` shows one that is, `moby/22941` shows one that is not (a shared mutable writer is not a move).

## 3. Parser gaps found while transliterating (cost, not correctness)

1. `interface` declarations parse-fail despite being specified — every kernel whose race crosses an interface cannot be transliterated faithfully.
2. Top-level `var` is rejected (declarations only); package-level mutable state must be restructured into fields or `managed`.
3. Struct literals require named fields (`revision_watcher{dests_ch: ch}`); positional literals parse-fail.

These are the "restructure" costs the tripwire is meant to price, and they are real: three of three kernels hit at least one.

## 4. Next

Finish the remaining four single-spawn kernels, then fetch the upstream patch for each and transliterate the **fixed** form — the only form whose rejection counts as cost. Publish per-kernel: site shape, lifted?, fixed-form verdict, `managed` required (yes/no), parser gap hit.