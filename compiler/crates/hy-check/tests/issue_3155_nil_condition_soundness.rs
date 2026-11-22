//! issue_3155_nil_condition_soundness.rs — regression tests for #3155 (Row 13 BUILDABLE).
//!
//! Untyped `nil` in `if`, `for`, and `match guard` statements passed `hyb check`
//! with 0 diagnostics because `kind_of_expr` evaluates `LitKind::Nil` to `OpKind::Unknown`,
//! and `check_bool_condition` permitted `OpKind::Unknown`.
//!
//! Go refuses untyped `nil` in conditions (`non-boolean condition in if/for statement`),
//! so `hyb check` must reject untyped `nil` conditions fail-closed as `HYB-E1002`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_if_nil_condition_rejected() {
    let cases = [
        ("bare nil", "fn main() {\n    if nil {}\n}\n"),
        ("parenthesized nil", "fn main() {\n    if (nil) {}\n}\n"),
        ("nested parenthesized nil", "fn main() {\n    if ((nil)) {}\n}\n"),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected Reject for {name}, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_for_nil_condition_rejected() {
    let cases = [
        ("bare nil in for", "fn main() {\n    for nil {}\n}\n"),
        ("parenthesized nil in for", "fn main() {\n    for (nil) {}\n}\n"),
        ("nested parenthesized nil in for", "fn main() {\n    for ((nil)) {}\n}\n"),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected Reject for {name}, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_match_guard_nil_condition_rejected() {
    let cases = [
        (
            "bare nil in match guard",
            "fn main() {\n    x := 0\n    match x {\n        0 if nil: {}\n        _: {}\n    }\n}\n",
        ),
        (
            "parenthesized nil in match guard",
            "fn main() {\n    x := 0\n    match x {\n        0 if (nil): {}\n        _: {}\n    }\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected Reject for {name}, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn test_valid_bool_conditions_accepted() {
    let cases = [
        ("bool literal true in if", "fn main() {\n    if true {}\n}\n"),
        ("bool variable in if", "fn main() {\n    b := true\n    if b {}\n}\n"),
        ("comparison in if", "fn main() {\n    x := 10\n    if x > 0 {}\n}\n"),
        ("nil comparison in if", "fn main() {\n    var s []int\n    if s != nil {}\n}\n"),
        ("bool literal false in for", "fn main() {\n    for false {}\n}\n"),
        ("bool variable in for", "fn main() {\n    b := false\n    for b {}\n}\n"),
        ("comparison in match guard", "fn main() {\n    x := 0\n    match x {\n        0 if x == 0: {}\n        _: {}\n    }\n}\n"),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected Accept for {name}, got {:?}",
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "expected no errors for {name}, got {:?}",
            r.diagnostics
        );
    }
}
