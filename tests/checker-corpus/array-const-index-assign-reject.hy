// ============================================================================
// array-const-index-assign-reject — Checker corpus (issue #999).
//
// PURPOSE: a constant index past a fixed-size array's length on the
// ASSIGNMENT-target place (`a[300] = 1`), not in expression position.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`. Go refuses the emitted store:
//   invalid argument: index 300 out of bounds [0:2]
//
// ---- Why this is a REJECT ----
// The #910 rule (array-const-index-past-length-reject.hy) bounded constant
// indices in EXPRESSION position: `check_index_kind` resolved the base's
// declared array length from the type. The assignment path read the same
// rule through `place_read`, which passed no base expression — the length
// lookup was silently disabled for every store form. The write path now
// mirrors the base place into expression form, so `a[300] = 1` resolves the
// declared length exactly like the read `a[5]` did.
//
// Found by the W-023 Go-toolchain oracle (wildcard #565, spike #577),
// INVARIANT-4, cases 86/140/152 (`index_and_bounds` family); re-verified on
// main d7ab63a and ec3fe0b on 2026-10-20/21.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired controls (must stay clean) ----
// `a[1] = 1` below is the in-bounds local control. The SLICE fence is the
// sibling `control-slice-index-past-length.hy`: a slice's length is not part
// of its type, so a constant store past the current length is a RUNTIME
// panic in Go, not a compile error — bounding it would over-reject.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var a [2]int
    a[1] = 1                        // in-bounds store: stays clean
    a[300] = 1                      // out of bounds: constant index 300 on a [2]int
    fmt.println("{a[0]}")
}
