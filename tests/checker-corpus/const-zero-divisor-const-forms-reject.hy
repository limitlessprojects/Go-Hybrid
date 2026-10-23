// ============================================================================
// const-zero-divisor-const-forms-reject — Checker corpus (issue #758).
//
// PURPOSE: zero divisors in DERIVED constant forms — a named file-level
// `const`, a folded float expression, `len()` of a constant string, and a
// zero divisor inside an array-length expression — not just the literal
// `0` twin the #149 rule already caught.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-
// accepted file must build cleanly with `go build`. Go folds constant
// expressions first and refuses all of these at compile time:
//   invalid operation: division by zero
//
// ---- Why this is a REJECT ----
// The divisor predicate only inspected inline literals (`is_int_literal_zero`,
// `is_float_literal_zero`), so named consts, folded float expressions,
// constant conversions, constant-string `len()`, and array-length positions
// passed with 0 diagnostics. The fix makes the predicate const-map-aware
// (#998 pattern: resolve through the file const map, shadow-checked) and
// mirrors it onto the array-length site (#999 pattern: same predicate,
// uncovered position). A runtime divisor (`10 / n`) still panics at run
// time and stays accepted; a local shadowing the const name is a runtime
// binding and stays accepted too.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/const_zero_divisor_const_forms.rs`.
// ============================================================================

const zero = 0
const s_val = "abc"

fn main() {
    x := 10 / zero
    _ = 1.0 / (1.0 - 1.0)
    y := 10 / (len(s_val) - 3)
    var a [1 / 0]int
    _ = x
    _ = y
    _ = a
}

// EXPECTED-VERDICT: REJECT BUILDABLE
