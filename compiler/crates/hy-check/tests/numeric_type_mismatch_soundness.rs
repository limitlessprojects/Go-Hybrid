//! numeric_type_mismatch_soundness.rs — tests for HYB-E1002 rejection of
//! arithmetic, comparison, equality, and compound assignment operations
//! between mismatched concrete numeric types (int vs float64 variables).
//!
//! Closes #1969 and #1970 (Row 13 BUILDABLE invariant).

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_arithmetic_between_int_and_float_variables_1969() {
    // Subtraction
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = x - f\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = f - x\n}\n");

    // Multiplication
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = x * f\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = f * x\n}\n");

    // Division
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = x / f\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = f / x\n}\n");

    // Addition
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = x + f\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    _ = f + x\n}\n");
}

#[test]
fn rejects_comparison_between_int_and_float_variables_1969() {
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if x < f {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if f < x {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if x <= f {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if f <= x {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if x > f {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if f > x {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if x >= f {}\n}\n");
    assert_e1002("fn main() {\n    x := 10\n    f := 2.5\n    if f >= x {}\n}\n");
}

#[test]
fn rejects_equality_between_int_and_float_variables_1970() {
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    if f == i {}\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    if i == f {}\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    if f != i {}\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    if i != f {}\n}\n");
}

#[test]
fn rejects_compound_assign_between_float_and_int_variable_1970() {
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    f += i\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    f -= i\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    f *= i\n}\n");
    assert_e1002("fn main() {\n    f := 3.14\n    i := 2\n    f /= i\n}\n");
}

#[test]
fn accepts_untyped_int_constants_with_float_variables() {
    // Untyped int constants adapt to float in Go
    assert_ok("fn main() {\n    f := 2.5\n    _ = f + 1\n    _ = 1 + f\n    _ = f - 1\n    _ = 1 - f\n    _ = f * 2\n    _ = 2 * f\n    _ = f / 2\n    _ = 2 / f\n}\n");
    assert_ok("fn main() {\n    f := 2.5\n    if f == 1 {}\n    if 1 == f {}\n    if f != 1 {}\n    if 1 != f {}\n    if f < 2 {}\n    if 2 < f {}\n    if f <= 2 {}\n    if 2 <= f {}\n    if f > 2 {}\n    if 2 > f {}\n    if f >= 2 {}\n    if 2 >= f {}\n}\n");
    assert_ok("fn main() {\n    f := 2.5\n    f += 1\n    f -= 1\n    f *= 2\n    f /= 2\n}\n");
}

#[test]
fn accepts_matching_numeric_variables() {
    assert_ok("fn main() {\n    x := 10\n    y := 20\n    _ = x + y\n    _ = x - y\n    _ = x * y\n    _ = x / y\n    if x < y {}\n    if x == y {}\n}\n");
    assert_ok("fn main() {\n    f1 := 1.5\n    f2 := 2.5\n    _ = f1 + f2\n    _ = f1 - f2\n    _ = f1 * f2\n    _ = f1 / f2\n    if f1 < f2 {}\n    if f1 == f2 {}\n}\n");
}
