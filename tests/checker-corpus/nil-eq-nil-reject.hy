// ============================================================================
// nil-eq-nil-reject — Checker corpus (issue #1567).
//
// PURPOSE: `nil == nil` — neither operand has a type, and Go rejects the
// comparison outright. Same early return in `check_equality` as
// nil-compare-non-nilable-reject, with no declared type to report against,
// so it lands on the untyped-nil leg (E4003) instead of E4001.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE).
//
// ---- Why this is a REJECT ----
// Go: `invalid operation: nil == nil (operator == not defined on untyped nil)`.
// nil has exactly two homes (N1) — `error` and the foreign-Go perimeter — and
// both must be visible in a DECLARED type. Two bare nils name neither.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT N1
// EXPECTED-DIAGNOSTIC: HYB-E4003
// ============================================================================

fn main() {
    if nil == nil {
        fmt.println("both")
    }
}
