// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Pre-registered 2026-10-15 before running this fixture against the checker.
// Rationale: issue #134 form 3. `x, x := 1, 2` repeats a name on the left
// of one `:=`; Go rejects the emitted statement with `x repeated on left
// side of :=`. Rejected fail-closed as HYB-E1002 (rule BUILDABLE).
fn main() {
    x, x := 1, 2
    fmt.println(x)
}
