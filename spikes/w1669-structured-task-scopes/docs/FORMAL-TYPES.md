# Formal Type Rules for Structured Task Scopes

This document specifies the formal typing rules for `scope { ... }` blocks and affine `JoinHandle[T]` in Hybrid.

## Notation

We use standard type judgment forms:

- `Γ ⊢ e : τ` — expression `e` has type `τ` in context `Γ`
- `Γ ⊢ e ⇓ v` — expression `e` evaluates to value `v` in context `Γ`
- `Γ ⊢ s ✓` — statement `s` is well-typed in context `Γ`

## Handle States

Each affine handle `h` has a state in the lattice:

```
UNJOINED ⊑ CONSUMED
```

The context `Γ` maps handle names to their states:

```
Γ = { h₁ ↦ UNJOINED, h₂ ↦ CONSUMED, ... }
```

## Typing Rules

### Rule 1: Task Spawn (T-Spawn)

```
Γ ⊢ f : (τ₁, ..., τₙ) → τ    Γ ⊢ aᵢ : τᵢ    h ∉ dom(Γ)
────────────────────────────────────────────────────────────
Γ ⊢ h := task f(a₁, ..., aₙ) ✓    Γ' = Γ[h ↦ UNJOINED]
```

**Meaning:** Spawning a task creates a new affine handle in state `UNJOINED`.

### Rule 2: Join (T-Join)

```
Γ(h) = UNJOINED
────────────────────────────────────
Γ ⊢ join(move h) ✓    Γ' = Γ[h ↦ CONSUMED]
```

**Meaning:** Joining an `UNJOINED` handle consumes it, transitioning to `CONSUMED`.

### Rule 3: Cancel (T-Cancel)

```
Γ(h) = UNJOINED
────────────────────────────────────
Γ ⊢ cancel(move h) ✓    Γ' = Γ[h ↦ CONSUMED]
```

**Meaning:** Cancelling an `UNJOINED` handle consumes it.

### Rule 4: Drain (T-Drain)

```
Γ(h) = UNJOINED
────────────────────────────────────
Γ ⊢ drain(move h) ✓    Γ' = Γ[h ↦ CONSUMED]
```

**Meaning:** Draining an `UNJOINED` handle consumes it.

### Rule 5: Double Consumption (T-Double)

```
Γ(h) = CONSUMED
────────────────────────────────────
Γ ⊢ join(move h) ✗    [HYB-E3502]
```

**Meaning:** Consuming an already-consumed handle is a type error.

### Rule 6: Scope Exit (T-Scope-Exit)

```
Γ ⊢ s ✓    Γ' = Γ after s    ∀h ∈ dom(Γ'). Γ'(h) = CONSUMED
────────────────────────────────────────────────────────────
Γ ⊢ scope { s } ✓
```

**Meaning:** A scope is well-typed only if all handles are `CONSUMED` at exit.

### Rule 7: Scope Exit with Leak (T-Scope-Leak)

```
Γ ⊢ s ✓    Γ' = Γ after s    ∃h ∈ dom(Γ'). Γ'(h) = UNJOINED
────────────────────────────────────────────────────────────
Γ ⊢ scope { s } ✗    [HYB-E3501]
```

**Meaning:** A scope with unjoined handles at exit is a type error.

### Rule 8: Foreign Callback (T-Foreign)

```
Γ ⊢ f : foreign_callback    h := task g(...) inside f    managed(...) ∉ scope
────────────────────────────────────────────────────────────
Γ ⊢ h := task g(...) ✗    [HYB-E3503]
```

**Meaning:** Spawning inside a foreign callback without `managed` permit is a type error.

## Affine Properties

### Property 1: Affine Usage

Each handle is used **at most once**:

```
∀h. |{i : h is consumed at step i}| ≤ 1
```

### Property 2: Lifetime Containment

All handles spawned in a scope are consumed before scope exit:

```
∀h spawned in scope S. h is consumed before exit(S)
```

### Property 3: No Escape

Handles cannot escape their lexical scope:

```
h spawned in scope S ⟹ h ∉ dom(Γ) after exit(S)
```

## Soundness Theorem

**Theorem (Structured Concurrency Soundness):** If `Γ ⊢ scope { s } ✓`, then:
1. Zero goroutine leaks: all tasks spawned in `s` are joined/cancelled/drained
2. Zero use-after-free: no handle is consumed twice
3. Zero foreign escapes: all foreign callback spawns are managed

**Proof sketch:** By induction on the typing derivation. Rules T-Join, T-Cancel, T-Drain ensure single consumption. Rule T-Scope-Exit ensures all handles are consumed before exit. Rule T-Foreign ensures foreign spawns are managed. ∎

## Comparison with Linear Types

Our affine handles are **affine**, not linear:
- **Linear**: must be used **exactly once**
- **Affine**: may be used **at most once** (can be dropped/ignored)

In our system, handles can be implicitly dropped at scope exit if they are `UNJOINED`, but this triggers a diagnostic (HYB-E3501). This is a **soft affine** system: the type system allows dropping, but the checker enforces usage.

## Integration with Ownership Model

Structured task scopes integrate with Hybrid's ownership model (SPEC §9):

1. **Sendability at spawn**: Arguments to `task f(...)` are sendability check-sites (C5)
2. **No shared mutable state**: Handles are affine, preventing shared mutable handle state
3. **Boundary rule**: Handles cannot cross the safe/unsafe boundary

## Future Work: Dependent Types

Extension to dependent types could allow:
- **Bounded scopes**: `scope<10> { ... }` for at most 10 concurrent tasks
- **Typed handles**: `JoinHandle[Result, Error]` for typed results
- **Timeout types**: `JoinHandle[T, Timeout]` for timeout-aware handles

## References

- SPEC-LANGUAGE.md §8.6 — Surface syntax and diagnostics
- GUARANTEES.md Row 12 — Compile-time guarantee statement
- spikes/w1669-structured-task-scopes/ — Reference implementation
- Nathaniel J. Smith — Trio/structured concurrency in Python
- Martin Sustrik — libmill/libdill structured concurrency in C
