// ============================================================================
// control-slice-index-past-length — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: a SLICE index past the length. This is the case most likely to be
// over-rejected while fixing the fixed-array form: Go rejects `a[300]` on
// `[2]int` AT COMPILE TIME but accepts `xs[300]` on a slice, because a
// slice's length is not part of its type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) is satisfied — the emitted
// Go compiles. Hybrid makes no claim about run-time bounds panics, the same
// position it takes for send-on-closed-channel (Row 16, "No claim").
//
// ---- Why this is an ACCEPT ----
// The checker's own bounds work must distinguish a fixed ARRAY from a SLICE.
// `xs := []int{1, 2}` followed by a constant `xs[300]` compiles; rejecting it
// would be an over-reach into a property Hybrid does not claim. This control
// exists so the array fix cannot be written as a blanket index bound check.
//
// ---- Why the access is guarded ----
// The differential loop's accept-side contract is stronger than Row 13: an
// accepted program must also RUN with exit 0 (tools/differential_loop.py:
// "A checker accept that cannot execute is a false positive"). An unguarded
// `xs[300]` is legal Go that panics at run time, so this fixture would fail
// the loop's run half.
//
// The corpus reserves `// RUNTIME-HAZARD:` markers for hazards that cannot be
// avoided — corpus 02's deliberate concurrent map write is one. This hazard
// IS avoidable: wrapping the access in a branch that is false at run time
// keeps the constant index fully visible to the checker (a naive
// "constant index >= length" rule still fires on it, which is what this
// fixture must fence) while the program exits 0.
//
// ---- Expected verdict (verified on this branch before committing) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — 0 annotations, 0 managed. `hyb check` accepts it, the emitted Go
//   compiles, and the binary runs with exit 0; all three were run before this
//   fixture was committed.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    xs := []int{1, 2}
    if len(xs) > 300 {
        fmt.println("{xs[300]}")   // compile-only: constant index past the length
    }
    fmt.println("{len(xs)}")
}
