// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: 7
// ============================================================================
// match-subject-type-guard-option-controls-accept — Checker corpus.
//
// PURPOSE: the ACCEPT control for the HYB-E5002 subject-type guard. The same
// trigger that rejected the scalar twin — `some(v)`/`none` arms — stays legal
// on a subject that resolves to `option[T]` (T20): both pattern families are
// defined there, exhaustiveness lives below as before, and the emitter
// lowers it to the `.ok`/`.val` machinery the Go compiler accepts. An
// over-eager version of the guard that rejects every variant-pattern match
// (instead of resolving the SUBJECT type first) breaks this program — this
// is exactly the failure mode the first draft of the fix hit (three
// pre-existing E5001 tests began emitting E5002); the twin pins it shut.
//
// RULE CITATION: SPEC-LANGUAGE T13/T14 (exhaustive option match), T20
// (Option API), GUARANTEES.md Row 13 (BUILDABLE — builds and prints 7).
// Registered 2026-10-25 with the HYB-E5002 fix, before the checker run.
package main

import "fmt"

fn main() {
    var x option[int] = some(7)
    match x {
        some(v): fmt.println(v)
        none:    fmt.println(0)
    }
}
