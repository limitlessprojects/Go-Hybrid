// ============================================================================
// match-arm-binding-redeclare-accept — hy-emit codegen fix, BUILDABLE (Row 13).
//
// PURPOSE: pin the BUILDABLE guarantee for match arms whose top level
// redeclares the arm's pattern binding. hy-check treats a match arm as a
// nested scope (hy-check/src/buildable.rs pushes a scope per arm and runs the
// body `block_scoped`), so `some(v): { v := v + 1 ... }` is a legal
// arm-local shadow there. The emitter's option-match lowerings (both the
// simple unguarded chain and the has_unguarded_some per-arm path) injected
// `v := hyMatch0.val` into the SAME Go block as the inlined arm body, so the
// body's `v := v + 1` passed `hyb check` with 0 diagnostics and then failed
// `go build` with `no new variables on left side of :=`. Accepted-but-
// unbuildable — reproduced as a real go build failure first.
//
// FIX SHAPE (byte-exact discipline): the injection must stay in the arm's Go
// scope and ONLY a redefining arm body gains a nested `{ ... }` block —
// arms that do not redefine the pattern binding keep byte-identical output,
// so no accept-side digest drifts. A wrap-around-both spelling was tried
// first and rejected: with the injection inside the wrap, a body that
// redefines the binding WITHOUT reading it first (`some(v): { v := 5 ... }`)
// still fails `go build` with `v declared and not used`. `rangebind::
// body_reads` is shadow-aware (a read after a top-level redeclare of the
// binding does not count), which is exactly the gate the injection needs.
//
// ---- Why this is an ACCEPT ----
//  1. shadow lifting: match arm bodies are nested scopes; `define_elidable`
//     bindings may be arm-shadowed at the body's top level.
//  2. `rangebind::expr_reads`/`body_reads` already describe which reads the
//     injections serve — no new checker rule, no new E-code.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

package main

import "fmt"

fn main() {
    ok := false
    r1 := 0
    r2 := 0
    r3 := 0
    // simple unguarded chain: the redefining arm body wraps, the injection
    // stays outside, and the arm-local shadow feeds the outer slot (3).
    o := some(2)
    ok = o.is_some()
    match o {
        some(v): { v := v + 1
            r1 = v }
        none: r1 = 0
    }
    // has_unguarded_some path (3 reachable arms): same treatment on the
    // unguarded redefining arm `(w)`; the guarded neighbour nests inside its
    // guard block on its own and keeps winning (3).
    o2 := some(3)
    match o2 {
        some(v) if v > 0: r2 = v
        some(w): { w := w + 1
            r2 = w }
        none: r2 = 0
    }
    // control: a redefining arm that never reads the pre-shadow binding
    // feeds the fixed arm-local constant (5) with no injected binding at all.
    o3 := some(9)
    match o3 {
        some(v): { v := 5
            r3 = v }
        none: r3 = 0
    }
    if ok {
        fmt.println(ok, r1, r2, r3)
    } else {
        fmt.println(0, 0, 0, 0)
    }
}

// EXPECTED-OUTPUT: true 3 3 5
