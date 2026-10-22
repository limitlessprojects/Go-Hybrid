// ============================================================================
// range-bound-type-forms-reject — Checker corpus (issue #827).
//
// PURPOSE: `for i in lo..hi` bounds that pass the shape fallback but fail
// the lowered Go: non-integer bounds, untyped const non-integer bounds,
// mismatched fixed-width integer types, and constants overflowing the
// counter's type.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — the emitter lowers
// ranges to `i := lo; i < hi; i++`; Go refuses every shape below
// (`mismatched types ...` / `constant ... overflows ...` /
// `truncated to int`).
//
// ---- Why this is a REJECT ----
// `non_integer_bound_kind` missed nil / bool-result operators / AddrOf,
// untyped const identifiers carry no type for `ty_of_expr`, and no rule
// compared the two bounds' integer types or folded constants against the
// counter's range. The fix extends the kind classifier, resolves untyped
// consts through the file const map (#998 pattern), and adds the
// lowering-derived mismatch/overflow rules to the Range arm. Untyped
// literals and untyped consts adapt and stay legal.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/range_bound_type_forms.rs`.
// ============================================================================

const f = 2.5

fn main() {
    for i in 0..nil { _ = i }
    for i in 0..(1 < 2) { _ = i }
    for i in 0..f { _ = i }

    var n u32 = 5
    for i in 0..n { _ = i }

    var lo i32 = 0
    var hi i64 = 5
    for i in lo..hi { _ = i }

    for i in 0..(1 << 63) { _ = i }
}

// EXPECTED-VERDICT: REJECT BUILDABLE
