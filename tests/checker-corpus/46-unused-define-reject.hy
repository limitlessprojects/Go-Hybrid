// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// Pre-registered 2026-10-15 before running this fixture against the checker.
// Rationale: issue #134 form 1. `x := 5` is never read anywhere in the
// function, so the emitted Go is `declared and not used: x` — the module
// can never build. The accepted-subset contract is that an accepted
// program emits buildable Go, so the checker rejects fail-closed as the
// new code HYB-E1002 (rule BUILDABLE).
fn main() {
    x := 5
    fmt.println("halo")
}
