//! #3072 — `real()` and `imag()` result kinds must resolve to `Float`, not `Unknown`.
//!
//! Prior to this fix, `call_result_kinds` in `buildable.rs` omitted `"real"` and `"imag"`,
//! causing their result kind to fall through to `None` which resolves to `OpKind::Unknown`.
//! Because `kinds_conflict` is fail-open on `Unknown`, returning `real(...)` from a `string`
//! or `int` function, initializing `var s string = real(...)`, assigning `s = imag(...)`,
//! passing `real(...)` to `take_str(...)`, or operating `real(...) + "string"` was accepted
//! with 0 diagnostics by `hyb check`, while downstream `go build` rejected them with
//! mismatched type errors, violating GUARANTEES.md Row 13 (BUILDABLE).
//!
//! This suite verifies:
//!   - Mismatched returns (`return real(...)` from `fn() string` or `fn() int`) are rejected (HYB-E1002).
//!   - Mismatched returns (`return imag(...)` from `fn() string`) are rejected (HYB-E1002).
//!   - Mismatched variable initializations (`var s string = real(...)`) are rejected (HYB-E1002).
//!   - Mismatched assignments (`s = imag(...)`) are rejected (HYB-E1002).
//!   - Mismatched call arguments (`take_str(real(...))`) are rejected (HYB-E1002).
//!   - Mismatched arithmetic operands (`real(...) + "hello"`) are rejected (HYB-E1002).
//!   - Mismatched comparisons (`imag(...) == "hello"`) are rejected (HYB-E1002).
//!   - Valid float returns, assignments, and operations stay accepted (0 diagnostics).

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
fn test_real_return_mismatched_string_rejected() {
    let src = r#"
package test

fn bad_ret() string {
    return real(complex(1.0, 2.0))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_imag_return_mismatched_string_rejected() {
    let src = r#"
package test

fn bad_ret() string {
    return imag(complex(1.0, 2.0))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_real_return_mismatched_int_rejected() {
    let src = r#"
package test

fn bad_ret() int {
    return real(complex(1.0, 2.0))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_real_var_init_mismatched_rejected() {
    let src = r#"
package test

fn bad_init() {
    var s string = real(complex(1.0, 2.0))
    _ = s
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_imag_assign_mismatched_rejected() {
    let src = r#"
package test

fn bad_assign() {
    var s string
    s = imag(complex(1.0, 2.0))
    _ = s
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_real_call_arg_mismatched_rejected() {
    let src = r#"
package test

fn take_str(s string) {
    _ = s
}

fn bad_call() {
    take_str(real(complex(1.0, 2.0)))
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_real_arithmetic_mismatched_rejected() {
    let src = r#"
package test

fn bad_arith() {
    _ = real(complex(1.0, 2.0)) + "hello"
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_imag_equality_mismatched_rejected() {
    let src = r#"
package test

fn bad_eq() bool {
    return imag(complex(1.0, 2.0)) == "hello"
}
"#;
    assert_contains_code(src, "HYB-E1002");
}

#[test]
fn test_positive_controls_valid_real_imag_operations() {
    let src = r#"
package test

fn take_float(f float64) {
    _ = f
}

fn ok_ret_real() float64 {
    return real(complex(1.0, 2.0))
}

fn ok_ret_imag() float64 {
    return imag(complex(1.0, 2.0))
}

fn ok_ops() {
    var f float64 = real(complex(1.0, 2.0))
    f = imag(complex(1.0, 2.0))
    take_float(real(complex(1.0, 2.0)))
    take_float(imag(complex(1.0, 2.0)))

    _ = real(complex(1.0, 2.0)) + 1.5
    _ = imag(complex(1.0, 2.0)) + 1.5
    _ = real(complex(1.0, 2.0)) == 1.0
    _ = imag(complex(1.0, 2.0)) == 2.0
    _ = f
}
"#;
    assert_ok(src);
}
