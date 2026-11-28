# W-Spike Design — Externally-Synchronized Checked Obligations (`@[sync_guarded(mu)]`)

**Target Proposal:** limitlessprojects/Go-Hybrid#4071  
**Author:** `@marhabynova`  
**Classification:** Wildcard Track Feasibility Spike (OQ2 / Row 8 / Sendability Obligation Model)

---

## 1. Problem Statement & Motivation

Under Hybrid's current static verification rules (**GUARANTEES.md Row 8**), Tier 0 strictly rejects any struct containing mutable fields (such as `map[K]V`, slices, or pointers) crossing a task spawn or channel send site:

```hy
struct holder {
    mu sync.mutex
    m  map[string]int
}

fn worker(h *holder, done chan<- bool) {
    for _ in 0..50000 {
        h.mu.lock()
        h.m["x"] = h.m["x"] + 1
        h.mu.unlock()
    }
    done <- true
}
```

Even though the programmer has correctly guarded every access to `h.m` with `h.mu.lock() ... h.mu.unlock()` (idiomatic, race-free Go), `hyb check` rejects this program with:

```text
HYB-E3001: this struct is guarded by sync.mutex — race-free idiomatic Go,
but Tier 0 does not credit locks at spawn sites (QUESTIONS.md Q1, reading (c))
```

Today, the only escape hatch is `managed("worker-pool", "mutex")`, which completely disables static checking and discards all soundness guarantees.

Proposal **#4071** observes that this is not an inherent limitation of static analysis, but a **modelling gap**. Instead of treating "this struct has a mutex" as an unprovable sendability property of the struct, we can treat it as a **static access obligation** attached to the field.

---

## 2. The `@[sync_guarded(mu)]` Model

### 2.1 Struct Declaration & Field Binding
A struct field holding mutable state can be annotated with `@[sync_guarded(mu)]`:

```hy
struct holder {
    mu sync.mutex
    @[sync_guarded(mu)]
    m  map[string]int
}
```

The checker enforces two structural invariants at struct declaration time:
1. **Mutex Existence:** `mu` must be an existing field declared in the same struct. If `mu` does not exist, the checker rejects with **`HYB-E3002`**.
2. **Mutex Type:** `mu` must be a valid synchronization primitive (`sync.mutex` or `sync.rwmutex`).

### 2.2 Boundary Crossing (Spawn & Send Sites)
At task spawn (`go worker(...)`) and channel send (`ch <- s`) boundaries:
- Standard Tier 0 rules enforce that only sendable types (immutable values, unaliased unique moves, or channels) may cross.
- Under the `sync_guarded` extension, a struct pointer or value is permitted to cross a task boundary **if and only if** every mutable field is either:
  1. Proven unshared via ownership transfer (`move`), or
  2. Explicitly bound to a mutex field via `@[sync_guarded(mu)]`.
- If an unannotated mutable field remains, the checker rejects fail-closed with **`HYB-E3001`**.

### 2.3 Access Obligation Verification
Inside any function accessing `h.m`:
- The checker tracks the active **Lock Region State** $\mathcal{L} \subseteq \text{Path} \times \text{Field}$.
- A call `h.mu.lock()` or `h.mu.rlock()` adds `(h, mu)` to $\mathcal{L}$.
- A call `h.mu.unlock()` or `h.mu.runlock()` removes `(h, mu)` from $\mathcal{L}$.
- When an expression reads or writes `h.m`, the checker verifies that `(h, mu) \in \mathcal{L}`.
- If `h.m` is accessed while `(h, mu) \notin \mathcal{L}`, the checker rejects fail-closed with **`HYB-E3003`** (*access to guarded field without holding lock*).

---

## 3. Diagnostic Code Matrix

| Code | Trigger Condition | Severity |
| :--- | :--- | :--- |
| **`HYB-E3001`** | Struct with unannotated mutable field crossing spawn boundary | Fail-Closed Error |
| **`HYB-E3002`** | `@[sync_guarded(mu)]` specifies non-existent or invalid mutex field | Fail-Closed Error |
| **`HYB-E3003`** | Field access outside of an active lock acquisition region | Fail-Closed Error |
| **`HYB-E3004`** | Mutex unlocked while an active reference/borrow to guarded field is live | Fail-Closed Error |

---

## 4. Zero-Cost Lowering to Go

Because Hybrid compiles to native Go:
- `@[sync_guarded(mu)]` is a pure compile-time annotation that produces zero runtime wrapper types or indirection.
- The emitted Go struct contains the plain native fields:
  ```go
  type holder struct {
      mu sync.Mutex
      m  map[string]int
  }
  ```
- Method calls `h.mu.lock()` and `h.mu.unlock()` lower directly to `h.mu.Lock()` and `h.mu.Unlock()`.
- Resulting Go code compiles cleanly under `go build` and exhibits zero allocations beyond standard Go concurrency primitives.
