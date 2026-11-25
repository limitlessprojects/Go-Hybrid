// ============================================================================
// associated-fn-const-write-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: an ASSOCIATED function (`fn Type.name(...)`, called as
//   `Type.name(...)`) that writes through a map parameter. The callee is a
//   top-level function bound in `ck.fns` under the qualified name
//   `"Type.name"`, but the loan/interprocedural rules used to resolve only
//   `Expr::Ident` callees — a `Type.name(...)` call parses as
//   `Expr::Field { base: Ident(Type), name }` and fell through to the
//   method/unknown paths, so the const write was never seen (issue #4407).
//
// RULE CITATION: T22 (deep immutability of `const`). `shared` is a file-level
//   `const` map; `ns.mutate(shared)` stores into it through the callee's
//   parameter, which is a write to a `const` root — HYB-E3006.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `shared` (map header, const-frozen).
//   Layer 2 (Rust-side owned): none.
//
// Negative control (same semantics, plain top-level fn) has always been
// rejected: see 27-const-write-reject.hy. This fixture pins that routing the
// call through an associated function does not launder the same write.
// ============================================================================
//
// EXPECTED-VERDICT: REJECT IMMUT

import "fmt"

struct ns { tag int }

const shared = map[string]int{"k": 1}

fn ns.mutate(m map[string]int) {
    m["k"] = 99
}

fn main() {
    ns.mutate(shared)
    v := shared["k"]
    fmt.println("{v}")
}
