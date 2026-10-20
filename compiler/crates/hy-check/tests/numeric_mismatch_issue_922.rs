//! #922 — Binary arithmetic and relational operators permit mismatched numeric types (HYB-E1002).
//!
//! Go requires operands of `+`, `-`, `*`, `/`, `<`, `<=`, `>`, `>=`, `==`, `!=` to be
//! identical concrete types (unless one side is an untyped constant that adapts).
//! `hy-check` previously accepted `int` vs `float64` mismatches because it only checked
//! that both sides were "numeric" without verifying they were the same concrete kind.

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
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// Reject: `+` on mismatched concrete numeric types
// ---------------------------------------------------------------------------

#[test]
fn add_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x + f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn add_float_and_int_is_rejected() {
    assert_code(
        "fn main() {\n    var f float64 = 2.5\n    x := 10\n    _ = f + x\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// Reject: `-`, `*`, `/` on mismatched concrete numeric types
// ---------------------------------------------------------------------------

#[test]
fn sub_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x - f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn mul_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x * f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn div_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x / f\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// Reject: ordered comparisons on mismatched concrete numeric types
// ---------------------------------------------------------------------------

#[test]
fn lt_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x < f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn le_float_and_int_is_rejected() {
    assert_code(
        "fn main() {\n    var f float64 = 2.5\n    x := 10\n    _ = f <= x\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn gt_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x > f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn ge_float_and_int_is_rejected() {
    assert_code(
        "fn main() {\n    var f float64 = 2.5\n    x := 10\n    _ = f >= x\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// Reject: `==` and `!=` on mismatched concrete numeric types
// ---------------------------------------------------------------------------

#[test]
fn eq_int_and_float_is_rejected() {
    assert_code(
        "fn main() {\n    x := 10\n    var f float64 = 2.5\n    _ = x == f\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn ne_float_and_int_is_rejected() {
    assert_code(
        "fn main() {\n    var f float64 = 2.5\n    x := 10\n    _ = f != x\n}\n",
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// Accept: same-kind numeric operations
// ---------------------------------------------------------------------------

#[test]
fn add_int_and_int_is_accepted() {
    assert_ok(
        "fn main() {\n    x := 10\n    y := 20\n    _ = x + y\n}\n",
    );
}

#[test]
fn add_float_and_float_is_accepted() {
    assert_ok(
        "fn main() {\n    var f float64 = 2.5\n    var g float64 = 1.5\n    _ = f + g\n}\n",
    );
}

#[test]
fn lt_int_and_int_is_accepted() {
    assert_ok(
        "fn main() {\n    x := 10\n    y := 20\n    _ = x < y\n}\n",
    );
}

#[test]
fn eq_float_and_float_is_accepted() {
    assert_ok(
        "fn main() {\n    var f float64 = 2.5\n    var g float64 = 1.5\n    _ = f == g\n}\n",
    );
}

// ---------------------------------------------------------------------------
// Accept: string operations (not affected by this fix)
// ---------------------------------------------------------------------------

#[test]
fn add_string_and_string_is_accepted() {
    assert_ok(
        "fn main() {\n    s := \"hello\"\n    t := \"world\"\n    _ = s + t\n}\n",
    );
}

#[test]
fn lt_string_and_string_is_accepted() {
    assert_ok(
        "fn main() {\n    s := \"hello\"\n    t := \"world\"\n    _ = s < t\n}\n",
    );
}

// ---------------------------------------------------------------------------
// Known limitation: untyped numeric literals adapt between numeric families
// in Go (`f + 1`, `1 + 2.5`, `1 == 1.5`). The kind model cannot distinguish an
// untyped constant literal from a typed variable, so these are accepted even
// though one side is an int literal and the other is a float. This is a
// deliberate false-positive tradeoff; see issue #922 for discussion.
// ---------------------------------------------------------------------------
