//! issue_3164_option_method_arity.rs — issue #3164 (Row 13 BUILDABLE).
//!
//! option[T] methods `unwrap()`, `is_some()`, and `is_none()` accept 0 arguments.
//! `or(default)` accepts exactly 1 argument matching T.
//! Passing invalid argument counts must be rejected with HYB-E1002.

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
fn test_option_unwrap_with_arg_rejected() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.unwrap(123)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_option_is_some_with_arg_rejected() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.is_some(true)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_option_is_none_with_arg_rejected() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.is_none("extra")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_option_or_zero_args_rejected() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.or()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_option_or_too_many_args_rejected() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.or(1, 2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_option_valid_calls_accepted() {
    let src = r#"
fn main() {
    var o option[int]
    _ = o.unwrap()
    _ = o.is_some()
    _ = o.is_none()
    _ = o.or(42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

