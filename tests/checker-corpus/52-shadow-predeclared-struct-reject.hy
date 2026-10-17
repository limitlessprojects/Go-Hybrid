// ============================================================================
// 51-shadow-predeclared-struct-reject.hy - Checker corpus #51 (added WITH the
// fix; issue #326).
//
// PURPOSE: A user struct or type alias that shadows a predeclared primitive
// type name (such as `rune` or `string`) must be resolved as that struct or
// alias, not as the predeclared primitive.
// In issue #326, `struct rune { m map[string]string }` was resolved as
// TypeShape::Primitive because named_shape checked hardcoded primitive names
// before package structs. Since primitive values are sendable across tasks by
// copy, passing `rune` holding an un-synchronized map across tasks was wrongly
// accepted with 0 diagnostics.
//
// ---- Expected verdict (registered with the fix, 2026-10-17) ----
// EXPECTED-VERDICT: REJECT A1c
// EXPECTED-DIAGNOSTIC: HYB-E3001
//   REJECT - A1c (sendability): the value passed across task boundary is a
//   user-defined struct containing mutable map storage, which is not sendable.
//
// ---- Annotations / managed ----
// lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct rune {
    m map[string]string
}

fn worker(h rune, id string, done chan<- string) {
    h.m[id] = id
    done <- id
}

fn main() {
    h := rune{m: map[string]string{}}
    done := make(chan string)
    task worker(h, "a", done)
    task worker(h, "b", done)
}
