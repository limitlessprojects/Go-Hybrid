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
## 5. Remaining four single-spawn kernels (5 of 7 now have verdicts)

### kubernetes/80284 — two closures capturing the same `*authenticator`

Func-typed fields (`onRotate func()`) have no Hybrid spelling, so the field collapses to an `int`; the share, not the field type, is the question.

**Verdict: `HYB-E3001`** — "`&a` (*authenticator) is not sendable across tasks … mutable struct shared as a live borrow". For the buggy kernel this is a **true positive**: two tasks write `a.onRotate` concurrently.

### kubernetes/82550 — `go provider.LazyProvide()` x10 (method spawn)

Two restrictions fire, in order:

1. **`HYB-E3103`** — `struct field lazy_provider.actual has pointer type *caching_provider — fields never hold borrows`. The kernel's lazy-init pattern (`if p.actualProvider == nil { p.actualProvider = &CachingDockerConfigProvider{} }`) is unrepresentable as written.
2. With the field changed to a value, the method spawn itself: **`HYB-E3005`** — `p.lazy_provide` is a method value; O4.0 cannot prove it.

This kernel needs **two** restructures before it can even be scored.

### kubernetes/70892 — `go func(){ for piece := range toProcess { select { case <-stop: return; default: doWorkPiece(piece) } } }()`

Not transliterated: the closure captures `context.Context`, a `sync.WaitGroup`, a `<-chan struct{}`, and a **function-typed value** `DoWorkPieceFunc`. Hybrid has no `context`, no `WaitGroup`, and the parser has no function-type fields/params; three foreign subsystems would have to be modelled before the spawn site could be judged. Recorded as a **porting gap**, not a verdict.

### moby/18412 — `go func(){ exitErr := cmd.Wait(); done <- exitErr }()`

Not transliterated: the body is `os/exec` + `bytes.Buffer` + `time.After` + a select on a timer. All four are foreign subsystems with no Hybrid equivalent in the parser today. Recorded as a **porting gap**.

## 6. Scoreboard so far (7 single-spawn kernels)

| Kernel | Shape | Verdict | Counts as |
|---|---|---|---|
| moby/27037 | closure | OK after mechanical lift (`move` per-iteration) | **free** |
| moby/22941 | closure | E3001 shared mutable borrow | true positive (buggy) |
| serving/5865 | method | E3005 unprovable callee | **cost** |
| kubernetes/80284 | closure | E3001 shared mutable borrow | true positive (buggy) |
| kubernetes/82550 | method | E3103 pointer field + E3005 callee | **cost** (plus restructure) |
| kubernetes/70892 | closure | not transliterable (context / WaitGroup / func value) | **porting gap** |
| moby/18412 | closure | not transliterable (exec / bytes / time) | **porting gap** |

Of the seven: **1 lifts for free, 2 are correctly rejected on the buggy form, 2 are rejected for being method spawns regardless of bug or fix, 2 cannot be expressed at all without foreign subsystems.** Even before the fixed-form run, the method-spawn cost alone is 2/7 = 28.6%, above the O18 trigger.

## 7. Fixed-form verdicts (the number O18 actually asks for)

I fetched the upstream patches from the links in each kernel's README.

### moby/22941 — fix is "pass the capture as an argument"

The patch is four lines:

```diff
-		go func() {
-			srv.Write(pair[0])
-		}()
+		go func(x []byte) {
+			srv.Write(x)
+		}(pair[0])
```

This is precisely Hybrid's `move` idiom, written in Go by hand. **Fixed-form verdict: the capture is now a move; the remaining share is `srv` (the `*pipe`), which is mutex-guarded in the original.** Under A1 that share still triggers `E3001` — the mutex is invisible to the checker — so the fixed kernel needs `managed("guarded by wrMu","mutex")`. **Cost: one `managed` site on code that is correct.**

### kubernetes/80284 — fix adds a mutex around the shared struct

The patch adds `a.mu.Lock(); defer a.mu.Unlock()` around the mutation and changes `onRotate func()` to `onRotateList []func()`. The share is a `*Authenticator` across two tasks, now correctly guarded.

**Fixed-form verdict: `E3001` again** — "mutable struct shared as a live borrow". The guard is a runtime property; the spawn-callee proof sees only that two tasks hold the same pointer. So the fixed kernel needs `managed("guarded by a.mu","mutex")`. **Cost: one `managed` site on code that is correct.**

### What this establishes

For the two shared-borrow kernels, the *fixed* form is rejected exactly like the buggy one — the checker cannot see the mutex that makes the fix a fix. That is the O18 cost, and it is not a transliteration artifact: it follows from `managed` being the only channel for "guarded share", which is a design choice, not an oversight.

| Fixed-form outcome | Kernels |
|---|---|
| free after mechanical lift (`move`) | moby/27037, moby/22941's capture |
| rejected, needs `managed("…","mutex")` | moby/22941's `*pipe`, kubernetes/80284 |
| rejected for method spawn, fix-independent | serving/5865, kubernetes/82550 |
| not expressible | kubernetes/70892, moby/18412 |

So on the sample where a fixed form exists: **0 of 2 fixed kernels pass without the hatch**, and the two method-spawn kernels would fail regardless of the fix. The strict ruling's cost is not confined to buggy code — it lands on correct code that uses the two idioms Go programs use most: guarded shared state and method spawns.
