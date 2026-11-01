//! issue_2955_option_method_soundness.rs — tests for Issue #2955 (Row 13 BUILDABLE).
//!
//! Calling `.pop()` on an `option[T]` or `fn()` value must be rejected with HYB-E1002
//! (.pop() is only defined on slices []T).
//! Non-existent methods on option[T] (e.g. o.foo()) and incorrect option method
//! arities (e.g. o.unwrap(1), o.or(), o.or(1, 2)) must also reject with HYB-E1002.
//! Valid option method calls (unwrap(), is_some(), is_none(), or(default)) must pass.

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
fn invalid_option_and_fn_calls_rejected_e1002() {
    let cases = [
        (
            "pop_on_fn",
            r#"
fn takes_fn(f fn()) {
    f.pop()
}
"#,
        ),
        (
            "pop_on_option",
            r#"
fn main() {
    o := some(5)
    o.pop()
}
"#,
        ),
        (
            "nonexistent_method_on_option",
            r#"
fn main() {
    o := some(5)
    o.foo()
}
"#,
        ),
        (
            "unwrap_with_arg",
            r#"
fn main() {
    o := some(5)
    o.unwrap(1)
}
"#,
        ),
        (
            "is_some_with_arg",
            r#"
fn main() {
    o := some(5)
    o.is_some(2)
}
"#,
        ),
        (
            "is_none_with_arg",
            r#"
fn main() {
    o := some(5)
    o.is_none(2)
}
"#,
        ),
        (
            "or_under_arity",
            r#"
fn main() {
    o := some(5)
    o.or()
}
"#,
        ),
        (
            "or_over_arity",
            r#"
fn main() {
    o := some(5)
    o.or(1, 2)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should reject: {src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "case {name} should emit HYB-E1002, got: {codes:?}"
        );
    }
}

#[test]
fn valid_option_method_calls_accepted() {
    let cases = [
        (
            "unwrap_valid",
            r#"
fn main() {
    o := some(5)
    x := o.unwrap()
    _ = x
}
"#,
        ),
        (
            "is_some_and_is_none_valid",
            r#"
fn main() {
    o := some(5)
    if o.is_some() && !o.is_none() {
        fmt.println(1)
    }
}
"#,
        ),
        (
            "or_valid",
            r#"
fn main() {
    o := some(5)
    y := o.or(10)
    _ = y
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {name} should accept: {src}, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}
