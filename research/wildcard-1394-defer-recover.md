# Wildcard #1394 — Defer is not a channel, and `recover` proves it

**Issue:** [#1394](https://github.com/limitlessprojects/Go-Hybrid/issues/1394) · **Status:** design-only; no runtime changes in this PR.

## 0. The objection, stated plainly

The proposal converts `defer` into an affine exit-channel send — push a capture bundle, join at a synthetic function-exit consumer. That lowering cannot preserve `recover()`, and the reason is in the Go spec, not in an implementation detail: **`recover` returns non-nil only when called directly by a deferred function while that goroutine is panicking.** A deferred function reified as a message on a channel is executed by *whatever goroutine consumes the channel*. `recover()` there returns nil, the panic continues unwinding, and the program dies where it previously survived. The transformation is not semantics-preserving on the exact cases the proposal claims to make "statically decidable for the first time".

You can rescue it only by making the consumer run on the panic path of the same goroutine at the exact unwind point — which is a direct call, not a channel. At that point the exit channel is decorative: you have reinvented `defer` with extra steps and an IR node.

## 1. Measured scope of what would actually be deleted

The prompt says this "deletes all special-case `defer` analysis in `hy-check`". In `compiler/crates/hy-check/src/closures.rs` (2,102 lines, 39 lines mentioning `defer`):

| Function | Lines | Span |
|---|---|---|
| `check_deferred_uses` | 59 | 1543–1601 |
| `inspect_later_defer_fate` | 53 | 1602–1654 |
| `update_post_defer_watch` | 52 | 1791–1842 |

**164 lines total.** Against that: a new IR variant, producer lowering at every `defer` site, a consumer block on both the return and unwind CFG edges, channel allocation or a synthetic local, and the capture-bundle aliasing rules. Whatever the design's merits, "delete the heuristics" is a 164-line saving in a 2,102-line file — the win is being oversold, and the machinery is not free.

## 2. The capture-bundle contradiction

Deferred closures capture by **reference**: `defer func(){ x++ }()` observes the `x` at unwind time, not at defer time. Affine ownership is **move**: once a value enters the bundle it is consumed. The proposal needs one of:

- **Copy at defer time** — breaks the by-reference semantics that make `defer` useful (and double-runs destructors on owned values).
- **Alias at defer time** — the bundle holds a reference, so the capture is not affine and the "moved ownership boundary" the proposal promises to make safe is exactly the case it cannot encode.

Arguments are evaluated eagerly, closures are not, and the "capture bundle" fuses two rules with opposite evaluation times into one token.

Also: `closure_scoped_envs` is not a function — it is a field on the checker state (`model.rs:258`, populated at `model.rs:328`, read at `closures.rs:40`). The prompt cites it as `closures.rs:40-44`, which is a use site. That matters because the proposal is scoped as "delete these four things"; one of the four is a data structure the rest of the pass depends on.

## 3. `recover` has no model today, so this is a feature, not a refactor

Grepping the checker: `recover` appears **6 times**, all plumbing — an arity check (`builtins.rs:383`) and a type (`buildable.rs:2010` returns `interface{}`). There is no notion of "inside a deferred call", "during unwinding", or "returns nil otherwise". The proposal presents panic-unwind safety as a property unlocked by the refactor; in reality it is a new analysis that has to be built either way, and the channel encoding makes it *harder* because it severs the same-goroutine, same-frame relationship `recover` depends on.

## 4. What survives: a narrow, implementable rule set

Drop the IR rewrite. Keep the existing AST checks and add the panic-safety rule that is actually checkable without changing lowering:

1. **`recover` must be lexically reachable from a function literal passed to `defer`.** A `recover()` in an ordinary function body, or in a goroutine spawned by a deferred function, returns nil by spec — flag it. This is a local, path-insensitive check over the existing `closure_scoped_envs` mapping: no new IR, no channel.
2. **Deferred closure capture of a value moved into a `task`** — already tracked by `update_post_defer_watch` (1791); extend the watch to closure bodies rather than replacing the mechanism.
3. **Deferred `close(ch)` followed by a send on `ch` in the same function** — a real, common panic, checkable with the existing channel state.
4. **Defer inside a loop capturing the loop variable** — the classic; report the count of defers, not a linearization.

That is perhaps 60–80 lines added, zero lines of lowering changed, and it catches the bugs that motivate the proposal.

## 5. Pre-registered tests

1. `recover()` called from a plain function invoked by a deferred function → **must not** be treated as recovering (returns nil at runtime).
2. `recover()` directly in the deferred function literal → accepted.
3. `defer` closure capturing a value already moved to a `task` → error at the defer site, not at unwind.
4. `defer close(ch)` then `ch <- v` in the same block → error.
5. `for { defer f() }` → diagnostic naming unbounded defer accumulation.
6. The five existing `defer` corpus cases keep their verdicts; no new false positives on `benchmarks/workloads/hy/*.hy`.

**What does not survive:** the exit-channel encoding (breaks `recover`), "delete the defer heuristics" as a large win (164 measured lines), and the framing that panic-unwind safety falls out of the refactor (there is no `recover` model to fall out of).

**What survives:** a small panic-safety rule set on the existing walk, with six tests, that can land without touching lowering.
