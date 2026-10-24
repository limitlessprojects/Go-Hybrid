// ============================================================================
// binop-mismatched-numeric-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: binary arithmetic and ordered comparison between two mismatched
// concrete numeric types must not pass `hyb check`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// `check_binary_operands` compared numeric FAMILIES, not concrete kinds: for
// `< <= > >=` and `+` the mismatch term was `lk.is_numeric() != rk.is_numeric()`
// (both `int` and `float64` are numeric → never mismatched), and for `- * /`
// only kind membership was required. So `x + f`, `x - f`, `x * f`, `x / f`,
// `x < f`, `f <= x` all checked clean, while `go build` refused each with
// `invalid operation: ... (mismatched types int and float64)`.
//
// The fix routes cross-kind pairs through `numeric_adapt_conflict`, which
// allows exactly Go's two untyped-constant adaptation lanes — `1 + 2.5`
// (both untyped) and `f + 1` (int const → float64, including untyped `const`
// identifiers) — and rejects every typed-vs-typed or typed-vs-float-const
// pairing.
//
// Go: `invalid operation: x + f (mismatched types int and float64)`.
//
// ---- Expected verdict (reproduced as a real `go build` failure first) ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// The accept-side control lives in `hy-check/tests/
// binop_operand_types_issue_922.rs` (`add_float_var_and_int_literal_accepted`,
// `add_two_untyped_literals_accepted`, `string_concat_and_order_accepted`, …),
// so these rejections cannot be widened into a rule that refuses the
// adaptation spellings Go builds.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    x := 10
    var f float64 = 2.5
    _ = x + f          // arithmetic, int var vs float64 var
    _ = x - f
    _ = x * f
    _ = x / f
    _ = x < f          // ordered comparison, both directions
    _ = f <= x
    _ = x + 2.5        // int var vs untyped FLOAT constant: Go says
                       // mismatched types (the const cannot become int)
    fmt.println("{x}{f}")
}
