//! min_max_soundness.rs — regression tests for Issue #2892.
//!
//! Built-in `min` and `max` require all arguments to be of ordered types
//! (integers, floating-point numbers, or strings) and all arguments must
//! have matching, compatible types and identical concrete scalar types.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn min_mismatched_kind_int_float_rejected() {
    let src = r#"
fn test_min() {
    var a int32 = 1
    var b float64 = 2.0
    _ = min(a, b)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for int/float mismatch in min, got {:?}",
        res.diagnostics
    );
}

#[test]
fn min_mismatched_kind_int_str_rejected() {
    let src = r#"
fn test_min() {
    var a int32 = 1
    var s string = "x"
    _ = min(a, s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for int/string mismatch in min, got {:?}",
        res.diagnostics
    );
}

#[test]
fn min_mismatched_scalar_int32_int64_rejected() {
    let src = r#"
fn test_min() {
    var a int32 = 1
    var b int64 = 2
    _ = min(a, b)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for int32/int64 mismatch in min, got {:?}",
        res.diagnostics
    );
}

#[test]
fn max_mismatched_scalar_float32_float64_rejected() {
    let src = r#"
fn test_max() {
    var a float32 = 1.0
    var b float64 = 2.0
    _ = max(a, b)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for float32/float64 mismatch in max, got {:?}",
        res.diagnostics
    );
}

#[test]
fn min_unordered_bool_rejected() {
    let src = r#"
fn test_min() {
    var b1 bool = true
    var b2 bool = false
    _ = min(b1, b2)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for bool operand in min, got {:?}",
        res.diagnostics
    );
}

#[test]
fn max_unordered_slice_rejected() {
    let src = r#"
fn test_max() {
    var s1 []int32
    var s2 []int32
    _ = max(s1, s2)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for slice operand in max, got {:?}",
        res.diagnostics
    );
}

#[test]
fn min_max_ordered_valid_accepted() {
    let src = r#"
fn test_valid() {
    var a int32 = 1
    var b int32 = 2
    _ = min(a, b)
    _ = min(a, 10)

    var s1 string = "apple"
    var s2 string = "banana"
    _ = max(s1, s2)
    _ = max(s1, "cherry")

    var f1 float64 = 1.5
    var f2 float64 = 2.5
    _ = min(f1, f2)

    _ = min(1, 2, 3)
    _ = max(10, 20, 30)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected zero diagnostics, got {:?}", res.diagnostics);
}
