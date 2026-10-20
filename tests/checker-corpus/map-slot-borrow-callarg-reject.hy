// ============================================================================
// map-slot-borrow-callarg-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a map-slot borrow (`&m[k]`) used DIRECTLY as a call argument. The
// borrow has no lowering in that position: the emitter only turns an `&m[k]`
// into the O8 slot loan (copy the slot out, store it back at the binding's
// last use) when the borrow is the WHOLE value of a binding. In an argument
// or operand position it is emitted as written, and Go refuses to take the
// address of a map element:
//
//   invalid operation: cannot take address of e.services["a"] (map index expression)
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`; and Row 5 / O3, which spell the
// slot-loan form as `n := &m[k]` (the borrow is the binding's whole value).
//
// ---- Why this is a REJECT ----
// The program below was ACCEPTED by the checker before this rule and produced
// Go that `go build` refuses, i.e. an accept that cannot reach a running
// binary. The borrow is a loan only where a binding exists to store back to;
// the fix keeps `n := &m[k]` (and `n := (&m[k])`, parens are transparent)
// accepted and rejects the argument/operand positions with HYB-E3104.
//
// Scope line: the shape test is on the MAP (`map index expression` is not
// addressable in Go). Slice and fixed-array elements (`&xs[0]`, `&a[0]`) are
// addressable and keep their lowering; slice-element borrow semantics are a
// separate soundness track, not this rule.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT LOAN-ESCAPE
// EXPECTED-DIAGNOSTIC: HYB-E3104
//   First error HYB-E3104 at line 60, col 11 (`&e.services["a"]` in `touch`
//   argument position).
//
// ---- Paired fixture ----
// `map-slot-borrow-paths-accept.hy` holds the legal spellings — the binding
// initializer, the parenthesised binding, the re-binding assignment and the
// corrected call-argument pattern (copy the element out, pass `&copy`) — so
// this rejection cannot widen into a rule that refuses buildable programs.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct service {
    replicas int
    tag      string
}

struct env {
    services map[string]service
}

fn touch(s *service) {
    s.replicas = 9
}

fn f(e *env) {
    touch(&e.services["a"])
}

fn main() {
    e := env{services: map[string]service{"a": {replicas: 1, tag: "x"}}}
    f(e)
    v := e.services["a"].replicas
    fmt.println("r {v}")
}
