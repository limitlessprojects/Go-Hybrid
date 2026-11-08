// EXPECTED-VERDICT: REJECT T13
// EXPECTED-DIAGNOSTIC: HYB-E5002
// ============================================================================
// match-variant-arms-on-scalar-subject-reject — Checker corpus (soundness).
//
// PURPOSE: variant patterns on a subject that resolves to neither
// `option[T]` nor a declared enum.
//
// RULE CITATION: SPEC-LANGUAGE T13 (`match` arms/variants), T20 (Option API —
// `some`/`none` are the option constructors), GUARANTEES.md Row 13
// (BUILDABLE).
//
// ---- Why this is a REJECT ----
// Before the HYB-E5002 subject-type guard, the pass-2 option upgrade in
// `check_match` re-classed any match whose arms are `some`/`none` paths as an
// option match unconditionally: exhaustiveness passed with 0 diagnostics and
// the emitter lowered the arms to `hyMatchN.ok` / `hyMatchN.val` field
// accesses on the raw subject, so `go build` rejected the accepted program
// (`hyMatch0.ok undefined (type int has no field or method ok)`) — accepted-
// but-unbuildable (row 13). Variant machinery is defined ONLY for `option[T]`
// (T20) and declared enums (T13/T14); a known non-option, non-enum subject
// rejects fail-closed instead of accepting unbuildable Go.
//
// Controls: the literal-arm twin
// (match-variant-arms-scalar-subject-literal-twin-accept.hy) pins the accept
// half — same subject shape, scalar semantic arms — and
// `match_option_exhaustive-*` style accept fixtures pin option subjects.
//
// Registered 2026-10-25 with the HYB-E5002 fix (issue: match subject type vs
// variant patterns), before the checker run (hand-derived from T13/T20 text).
package main

import "fmt"

fn main() {
    var n int
    match n {
        some(v): fmt.println(v)
        none:    fmt.println(0)
    }
}
