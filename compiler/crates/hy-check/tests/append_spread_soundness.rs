//! append_spread_soundness.rs — regression tests for Issue #2884.
//!
//! Built-in `append(dst, src...)` requires `src` to be a slice with an element type
//! assignable to `dst`'s element type (or `src` is `string` and `dst` is `[]byte`).

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
fn append_spread_mismatched_element_scalar_rejected() {
    let src = r#"
fn test_spread_scalar() {
    var dst []int32
    var src []int64
    dst = append(dst, src...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for mismatched element scalar, got {:?}",
        res.diagnostics
    );
}

#[test]
fn append_spread_mismatched_element_kind_rejected() {
    let src = r#"
fn test_spread_kind() {
    var dst []int32
    var src []string
    dst = append(dst, src...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for mismatched element kind, got {:?}",
        res.diagnostics
    );
}

#[test]
fn append_spread_string_to_non_byte_slice_rejected() {
    let src = r#"
fn test_spread_str_non_byte() {
    var dst []int32
    var s string = "hello"
    dst = append(dst, s...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for string spread to []int32, got {:?}",
        res.diagnostics
    );
}

#[test]
fn append_spread_non_slice_rejected() {
    let src = r#"
fn test_spread_non_slice() {
    var dst []int32
    var n int32 = 1
    dst = append(dst, n...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for non-slice spread, got {:?}",
        res.diagnostics
    );
}

#[test]
fn append_spread_same_element_type_accepted() {
    let src = r#"
fn test_spread_valid() {
    var dst []int32
    var src []int32
    dst = append(dst, src...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid same-element slice spread should pass: {:?}",
        res.diagnostics
    );
}

#[test]
fn append_spread_string_to_byte_slice_accepted() {
    let src = r#"
fn test_spread_byte_valid() {
    var dst []byte
    var s string = "hello"
    dst = append(dst, s...)
    _ = dst
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid string spread to []byte should pass: {:?}",
        res.diagnostics
    );
}
