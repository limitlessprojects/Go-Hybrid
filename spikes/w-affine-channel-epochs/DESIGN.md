# Design: Static Channel Epoch Ordering & Affine Close Capability (Proposal #3776)

## 1. Problem Statement & Assumption Under Attack

Hybrid's `GUARANTEES.md` Row 16 currently states:
> *"Send-on-closed-channel panics: No claim — a liveness/protocol property, out of Tier-0's scope."*

In Go and current Hybrid, channels are uncounted, bidirectional handles. When multiple goroutines communicate, closing a channel is a high-risk operation:
1. **Send-on-closed panic:** If any sender attempts `ch <- v` after `close(ch)`, the Go runtime crashes with `panic: send on closed channel`.
2. **Double-close panic:** Calling `close(ch)` on an already closed channel crashes with `panic: close of closed channel`.
3. **Circular Select Deadlock:** Two or more tasks waiting on mutually dependent unbuffered channel sends and receives in nested loops/select blocks cause silent runtime deadlock.

Wildcard Proposal **#3776** attacks the assumption that communication safety and protocol liveness must remain runtime-only hazards. It proves that by treating the sender side as an **affine linear capability** within Hybrid's existing Polonius dataflow engine and assigning **static topological epoch tiers**, send-on-closed panics, double-close panics, and circular select deadlocks can be eliminated at compile time with **0 runtime overhead**.

---

## 2. Core Architecture

### 2.1 Affine Capability Decomposition
Channel instantiation `ch := make(chan T)` is decomposed into two distinct capability projections:
- `tx: affine send_cap[T]` — linear, non-duplicable ownership capability governing send (`tx <- v`) and close (`close(move tx)`).
- `rx: recv_cap[T]` — shareable, aliased capability governing receive (`<-rx`).

```hybrid
// Channel creation yields dual capabilities
tx, rx := make(chan int, 1)

// Send requires live send_cap
tx <- 42

// Close strictly consumes ownership via move
close(move tx)

// Any subsequent send or close triggers HYB-E3001 (use-after-move)
// tx <- 10  <-- REJECTED at compile time: HYB-E3001
// close(tx) <-- REJECTED at compile time: HYB-E3001
```

### 2.2 Multi-Producer Structured Splitting
When multiple concurrent tasks need to send to the same channel, `tx` cannot be casually copied. Instead, it must be split via an explicit structured capability:
```hybrid
tx1, tx2 := tx.split()
task producer1(move tx1)
task producer2(move tx2)
```
- Individual split tokens `tx1` and `tx2` carry `send` permission, but **not** independent `close` permission.
- Attempting to close an individual split branch while peer producers are active is rejected at compile time with `HYB-E3201` (`uncoordinated split close: channel close requires joined capability`).
- The channel is closed only upon lexical join of all split sub-capabilities or via an explicit orchestrator join handle.

### 2.3 Static Wait-For Epoch Ordering ($T(ch) \in \mathbb{N}$)
To guarantee deadlock freedom across multi-channel coordination sites, every channel is statically assigned a topological tier:
$$T(ch) \in \mathbb{N}$$
- When a task holds an unfulfilled dependency on $ch_A$ (e.g. inside a select arm or blocking loop) and initiates a communication on $ch_B$, the compiler requires:
$$T(ch_A) < T(ch_B)$$
- Any cyclic wait-for dependency ($T(ch_A) \le T(ch_B) \land T(ch_B) \le T(ch_A)$) is detected via cycle detection on the task's channel dependency graph and rejected with `HYB-E3202` (`circular wait-for protocol dependency detected`).

---

## 3. Zero-Cost Codegen Lowering (hy-emit)

A key requirement of Proposal #3776 is **zero allocation and zero wrapper overhead** in emitted Go code.
- `tx` and `rx` lower directly to native Go `chan T`:
  ```go
  // Emitted Go:
  ch := make(chan int, 1)
  ch <- 42
  close(ch)
  ```
- No mutexes, atomic state booleans, or wrapper structs are generated.
- The compile-time affine tracking vanishes completely in the emitted Go, achieving identical performance to raw Go channels (`0 B/op`, `0 allocs/op`).

---

## 4. Foreign Interoperability Hatch

Unchecked third-party Go channels (e.g. from `net/http` or external packages) that do not conform to affine capability rules are routed through:
```hybrid
managed("external-channel-interop", "interop") {
    // Unchecked raw Go channel interactions permitted here
}
```
Failing to declare `managed` triggers fail-closed diagnostic `HYB-E3005`.
