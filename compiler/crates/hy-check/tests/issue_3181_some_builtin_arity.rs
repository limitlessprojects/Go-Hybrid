//! issue_3181_some_builtin_arity.rs — issue #3181 (Row 13 BUILDABLE).
//!
//! Built-in `some(v)` takes exactly 1 argument (the value to wrap in an option)
//! and does not accept variadic spread `...`.
//! Invalid argument counts and spread calls must be rejected with HYB-E1002.

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
fn test_some_zero_arguments_rejected() {
    let src = r#"
fn main() {
    _ = some()
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
fn test_some_two_arguments_rejected() {
    let src = r#"
fn main() {
    _ = some(1, 2)
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
fn test_some_three_arguments_rejected() {
    let src = r#"
fn main() {
    _ = some(1, 2, 3)
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
fn test_some_variadic_spread_rejected() {
    let src = r#"
fn main() {
    var xs = []int{1, 2}
    _ = some(xs...)
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
fn test_some_valid_call_accepted() {
    let src = r#"
fn main() {
    _ = some(42)
    _ = some("hello")
    _ = some(true)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

