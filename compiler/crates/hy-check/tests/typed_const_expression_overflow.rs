//! #725 — a typed constant expression whose folded result overflows its type.
//!
//! Go folds constant expressions *before* the range check, so
//! `u8(255) + 1` is `constant 256 of type uint8 overflows uint8`. `hyb check`
//! accepted it with 0 diagnostics and the emitted Go failed `go build`,
//! breaking GUARANTEES Row 13 (BUILDABLE).
//!
//! The existing per-operand check could not see it: 255 fits `u8` and 1 fits
//! `u8`, so both operands passed and the **result** was never computed. Two
//! things were missing and both are needed:
//!
//!   1. `const_int_fold_with` had no `Expr::Call` arm, so `u8(255)` did not
//!      fold to a constant at all. Without this, a result-range check sees
//!      `NotConst` and stays silent — the fix would have looked correct and
//!      caught nothing.
//!   2. `check_binary_const_overflow` checked only operands, and skipped
//!      shifts entirely; the `Unary` arm had no range check at all.
//!
//! The accept half carries the load here, because the natural over-correction
//! is to reject *run-time* arithmetic on narrow types, which SPEC X1/X2
//! define as wrapping. `var x u8 = 200; x + 1` must keep building.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// The arithmetic operators, each on a typed constant.
// ---------------------------------------------------------------------------

#[test]
fn typed_constant_add_overflow_is_rejected() {
    assert_code(
        "const top u8 = 255\n\nfn main() {\n    a := top + 1\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
    assert_code(
        "fn main() {\n    a := u8(255) + 1\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
    // Both operands typed: 250 fits u8 and 10 fits u8, but 260 does not.
    assert_code(
        "fn main() {\n    a := u8(250) + u8(10)\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn typed_constant_sub_underflow_is_rejected() {
    assert_code(
        "fn main() {\n    a := u8(0) - 1\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn typed_constant_mul_overflow_is_rejected() {
    assert_code(
        "fn main() {\n    a := u8(200) * 2\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn typed_constant_shift_overflow_is_rejected() {
    // Shifts took an early return in the old check, so neither of these was
    // ever range-checked.
    assert_code(
        "fn main() {\n    a := u8(1) << 9\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
    assert_code(
        "fn main() {\n    b := i64(1) << 63\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn typed_constant_negation_overflow_is_rejected() {
    // `-u8(1)` is `constant -1 of type uint8 overflows uint8`.
    assert_code(
        "fn main() {\n    a := -u8(1)\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn signed_boundaries_are_rejected() {
    assert_code(
        "fn main() {\n    a := i8(127) + i8(1)\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
    // The classic MinInt64 idiom: Go folds `1 << 63` then refuses it.
    assert_code(
        "fn main() {\n    a := i8(-128) / -1\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E6002",
    );
}

#[test]
fn nested_and_ascribed_positions_are_rejected() {
    // Inside a conversion.
    assert_code(
        "fn main() {\n    c := int(u8(255) + 1)\n    fmt.println(\"{c}\")\n}\n",
        "HYB-E6002",
    );
    // As a typed `var` initializer.
    assert_code(
        "fn main() {\n    var b u8 = u8(255) + 1\n    fmt.println(\"{b}\")\n}\n",
        "HYB-E6002",
    );
}

// ---------------------------------------------------------------------------
// The accept half: everything here must stay ACCEPT. Each was verified to
// build before this fixture was committed.
// ---------------------------------------------------------------------------

#[test]
fn in_range_typed_constants_stay_accepted() {
    assert_ok("fn main() {\n    a := u8(255)\n    fmt.println(\"{a}\")\n}\n");
    assert_ok("fn main() {\n    a := u8(100) + u8(20)\n    fmt.println(\"{a}\")\n}\n");
    assert_ok("fn main() {\n    a := i64(1) << 62\n    fmt.println(\"{a}\")\n}\n");
}

#[test]
fn spec_x4_wrapping_conversion_stays_accepted() {
    // SPEC's own example: `u8(300)` is 44, not an error.
    assert_ok("fn main() {\n    a := u8(300)\n    fmt.println(\"{a}\")\n}\n");
}

#[test]
fn runtime_arithmetic_on_narrow_types_stays_accepted() {
    // SPEC X1/X2: a run-time value WRAPS. `x + y` is not a constant
    // expression, so Go compiles it and the wrap is defined behaviour.
    // Rejecting this would be the over-correction this test exists to catch.
    assert_ok(
        "fn main() {\n    var x u8 = 200\n    var y u8 = 100\n    a := x + y\n    fmt.println(\"{a}\")\n}\n",
    );
    assert_ok(
        "fn main() {\n    var x u8 = 255\n    a := x + 1\n    fmt.println(\"{a}\")\n}\n",
    );
}

#[test]
fn untyped_arithmetic_stays_accepted() {
    // Untyped constants take their type from context; these are plain ints
    // and fit comfortably.
    assert_ok("fn main() {\n    a := 1 + 2\n    fmt.println(\"{a}\")\n}\n");
    assert_ok("fn main() {\n    a := 1.5 + 2.5\n    fmt.println(\"{a}\")\n}\n");
}

#[test]
fn comparison_operands_are_not_range_checked() {
    // A comparison yields a bool, so an overflowing-looking operand pair is
    // not a value of the operand type.
    assert_ok(
        "fn main() {\n    a := u8(255) > 1\n    fmt.println(\"{a}\")\n}\n",
    );
}

#[test]
fn in_range_const_declaration_stays_accepted() {
    assert_ok("const top u8 = 255\n\nfn main() {\n    fmt.println(\"{top}\")\n}\n");
    assert_ok("const mid u8 = 100\n\nfn main() {\n    a := mid + 1\n    fmt.println(\"{a}\")\n}\n");
}
