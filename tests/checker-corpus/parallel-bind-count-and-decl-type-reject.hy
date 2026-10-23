// ============================================================================
// parallel-bind-count-and-decl-type-reject — Checker corpus (issue #905).
//
// PURPOSE: parallel declarations/assignments whose name and value counts
// differ, provably-single values bound to several names, typed local
// declarations initialized outside their declared type, and positional
// parallel assignments with conflicting kinds. Go refuses every shape
// below (`assignment mismatch` / `cannot use ... as ... value`).
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — HYB-E1002.
//
// ---- Why this is a REJECT ----
// `local` only counted for the single-name/single-value shape and never
// validated `l.ty` against initializers; `check_assign_operands` guarded
// its kind check behind `targets.len() == 1 && values.len() == 1`. The fix
// adds the parallel count guards, the provably-single-source binding
// guard, the typed-declaration initializer check, and the positional
// parallel-assign kind check (with the O8 slot-loan fail-open preserved).
// Valid counts, the untyped-int-to-float adaptation, and interface targets
// stay legal (accept controls pinned).
//
// Sibling take: harvested from issue #905 (magomedta) whose companion PR
// #906 rotted bot:stale-base — author credit magomedta. Issue #903 (also
// magomedta, companion PR #904 by rickymulya, stale-base) is fully
// subsumed by this landing and was closed with credit.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/binding_shape_guards_parallel.rs`.
// ============================================================================

fn main() {
    a, b, c := 1, 2
    _ = a
    _ = b
    _ = c

    d, e := 1, 2, 3
    _ = d
    _ = e

    var x int = "hello"
    _ = x

    let flag bool = 123
    _ = flag

    p := 1
    s := "a"
    p, s = s, p
    _ = p
    _ = s
}

// EXPECTED-VERDICT: REJECT BUILDABLE
