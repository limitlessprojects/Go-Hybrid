// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Pre-registered 2026-10-15 before running this fixture against the checker.
// Rationale: issue #134 form 1, the `_` sub-case called out in the report.
// `_ := 5` declares no new variable at all — Go rejects the emitted
// statement with `no new variables on left side of :=`. Rejected
// fail-closed as HYB-E1002 (rule BUILDABLE).
fn main() {
    _ := 5
    fmt.println("halo")
}
