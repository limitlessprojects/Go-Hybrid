// ============================================================================
// array-const-index-past-length-reject — Checker corpus (bounty track: "grow
// the adversarial corpus").
//
// PURPOSE: a constant index past the length of a FIXED-SIZE ARRAY. The emitted
// Go cannot build, so accepting it breaks GUARANTEES.md Row 13 (BUILDABLE).
//
// RULE CITATION: Row 13 — every `hyb check`-accepted file builds cleanly with
// `go build`. Go refuses the emitted `a[5]` on a three-element array:
//   invalid argument: index 5 out of bounds [0:3]
//
// ---- Why this is a REJECT ----
// `check_index_kind` bounded the constant index of the STRING form (`s[10]` on
// `const s = "abc"`) but not the fixed-array form: `a[5]` on `[3]int` checked
// clean and the emitted Go failed `go build`. An array's length is part of its
// type, so the bound is decidable statically — no run-time information is
// needed. Closed by the array branch of `buildable.rs::check_index_kind`
// (HYB-E1002) reading the declared length through `const_array_len_at`.
//
// ---- Why the length is read from the TYPE, not from the initializer ----
// `var a [3]int` is not a `const` binding, but its length is still certain.
// The string form's folder only trusts `const` bindings; the array form must
// not, or `var`/`let` arrays (the common shape) would stay unbounded.
//
// ---- Paired controls ----
// `control-slice-index-past-length.hy` is the SLICE twin and must stay ACCEPT:
// a slice's length is not part of its type, so `xs[300]` is legal Go and
// bounding it would over-reject. The two in-bounds lines below are the local
// control: only the `a[5]` line fires.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    var a [3]int
    a[2] = 7                        // in-bounds: stays clean
    fmt.println("{a[2]}")           // in-bounds read: stays clean
    fmt.println("{a[5]}")           // out of bounds: constant index 5 on a [3]int
}
