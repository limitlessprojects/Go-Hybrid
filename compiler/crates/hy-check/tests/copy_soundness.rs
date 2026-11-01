//! copy_soundness.rs — regression tests for builtin `copy` element type soundness (fixes #2686).
//!
//! Validates:
//! 1. `copy([]float64, []int)` rejected with HYB-E1002.
//! 2. `copy([]int, []float64)` rejected with HYB-E1002.
//! 3. `copy([]float64, []float64)` accepted cleanly.
//! 4. `copy([]int, []int)` accepted cleanly.
//! 5. String to byte slice copy `copy([]byte, string)` accepted cleanly.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_copy_float_from_int_slice_rejected() {
    let src = r#"
fn main() {
    var dst []float64
    var src []int
    copy(dst, src)
    _ = dst
    _ = src
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_copy_int_from_float_slice_rejected() {
    let src = r#"
fn main() {
    var dst []int
    var src []float64
    copy(dst, src)
    _ = dst
    _ = src
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_copy_same_float_slice_accepted() {
    let src = r#"
fn main() {
    var dst []float64
    var src []float64
    copy(dst, src)
    _ = dst
    _ = src
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}

#[test]
fn test_copy_same_int_slice_accepted() {
    let src = r#"
fn main() {
    var dst []int
    var src []int
    copy(dst, src)
    _ = dst
    _ = src
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}

#[test]
fn test_copy_string_to_byte_slice_accepted() {
    let src = r#"
fn main() {
    var dst []byte
    s := "hello"
    copy(dst, s)
    _ = dst
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}
