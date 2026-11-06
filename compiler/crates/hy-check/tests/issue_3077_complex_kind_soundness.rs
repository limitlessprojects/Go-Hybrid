//! #3077 — `complex64`/`complex128` and `complex()` result kinds must resolve to `Complex`, not `Unknown`.
//!
//! Prior to this fix, `kind_of_named` in `buildable.rs` omitted `"complex64"` and `"complex128"`,
//! and `call_result_kinds` omitted `"complex"`, causing complex types and `complex()` calls
//! to degrade to `OpKind::Unknown`. Because `kinds_conflict` is fail-open on `Unknown`,
//! returning complex values from string/int functions, initializing `var s string = complex(...)`,
//! assigning to strings/ints, passing to string/int parameters, and operating on complex + string
//! was accepted with 0 diagnostics by `hyb check`, while downstream `go build` rejected them with
//! mismatched type errors, violating GUARANTEES.md Row 13 (BUILDABLE).
//!
//! This suite verifies:
//!   - Mismatched returns from `complex()` calls are rejected (HYB-E1002).
//!   - Mismatched returns from `complex64`/`complex128` variables are rejected (HYB-E1002).
//!   - Mismatched variable initializations (`var s string = complex(...)`) are rejected (HYB-E1002).
//!   - Mismatched assignments (`s = complex(...)`) are rejected (HYB-E1002).
//!   - Mismatched call arguments (`take_str(complex(...))`) are rejected (HYB-E1002).
//!   - Mismatched arithmetic operands (`complex(...) + "hello"`) are rejected (HYB-E1002).
//!   - Mismatched comparisons (`complex(...) == "hello"`) are rejected (HYB-E1002).
//!   - Invalid ordered comparisons (`c1 < c2`) on complex types are rejected (HYB-E1002).
//!   - Valid complex returns, constants, assignments, and operations stay accepted (0 diagnostics).

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

fn assert_contains_code(src: &str, expected: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == expected),
        "expected diagnostic {expected}, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn test_complex_call_return_mismatched_string_rejected() {
    let src = r#"
package test

fn bad_ret() string {
    return complex(1.0, 2.0)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_call_return_mismatched_int_rejected() {
    let src = r#"
package test

fn bad_ret() int {
    return complex(1.0, 2.0)
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_var_return_mismatched_string_rejected() {
    let src = r#"
package test

fn bad_ret() string {
    var c complex128
    return c
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_var_return_mismatched_int_rejected() {
    let src = r#"
package test

fn bad_ret() int {
    var c complex64
    return c
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_var_init_mismatched_rejected() {
    let src = r#"
package test

fn bad_init() {
    var s string = complex(1.0, 2.0)
    _ = s
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_assign_mismatched_rejected() {
    let src = r#"
package test

fn bad_assign() {
    var s string
    s = complex(1.0, 2.0)
    _ = s
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_call_arg_mismatched_rejected() {
    let src = r#"
package test

fn take_str(s string) {
    _ = s
}

fn bad_call() {
    take_str(complex(1.0, 2.0))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_arithmetic_mismatched_rejected() {
    let src = r#"
package test

fn bad_arith() {
    _ = complex(1.0, 2.0) + "hello"
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_equality_mismatched_rejected() {
    let src = r#"
package test

fn bad_eq() bool {
    return complex(1.0, 2.0) == "hello"
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_ordered_comparison_rejected() {
    let src = r#"
package test

fn bad_cmp() bool {
    var c1 complex128
    var c2 complex128
    return c1 < c2
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_nil_comparison_rejected() {
    let src = r#"
package test

fn bad_nil() bool {
    var c complex128
    return c == nil
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_complex_conversion_mismatched_rejected() {
    let src = r#"
package test

fn bad_conv() {
    var c complex128
    var s string = string(c)
    _ = s
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_positive_controls_valid_complex_operations() {
    let src = r#"
package test

fn take_complex(c complex128) {
    _ = c
}

fn ok_ret_complex() complex128 {
    return complex(1.0, 2.0)
}

fn ok_ret_var() complex128 {
    var c complex128
    return c
}

fn ok_ops() {
    var c1 complex128 = complex(1.0, 2.0)
    var c2 complex128 = 1
    var c3 complex128 = 1.5
    c1 = complex(3.0, 4.0)
    take_complex(c1)

    _ = c1 + c2
    _ = c1 - c2
    _ = c1 * c2
    _ = c1 / c2
    _ = c1 == c2
    _ = c1 != c2
    _ = c3
}
"#;
    assert_ok(src);
}
