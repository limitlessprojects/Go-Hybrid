//! Regression tests for Tier-0 Rule O2 (`HYB-E3104` `LOAN-ESCAPE`) when a
//! `defer` statement stores a function-local borrow into a named result.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("defer_named_result_loan_escape.hy", src).expect("fixture must parse");
    let result = check_file(&ast);
    if let Some(code) = expected {
        assert_eq!(result.verdict, Verdict::Reject, "{:#?}", result.diagnostics);
        assert!(
            result.diagnostics.iter().any(|d| d.code == code),
            "expected {code}, got {:#?}",
            result.diagnostics
        );
    } else {
        assert_eq!(result.verdict, Verdict::Accept, "{:#?}", result.diagnostics);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }
}

#[test]
fn defer_storing_local_borrow_into_named_result_bare_return_rejects() {
    verify(
        r#"
type int_ptr = *int

fn escape_ptr() (out int_ptr) {
    x := 42
    defer fn() {
        out = &x
    }()
    return
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn defer_overwriting_explicit_return_with_local_borrow_rejects() {
    verify(
        r#"
type int_ptr = *int

fn escape_ptr(fallback *int) (out int_ptr) {
    x := 42
    defer fn() {
        out = &x
    }()
    return fallback
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn defer_capturing_late_rebound_borrow_into_named_result_rejects() {
    verify(
        r#"
type int_ptr = *int

fn escape_late(fallback *int) (out int_ptr) {
    x := 42
    var p int_ptr = fallback
    defer fn() {
        out = p
    }()
    p = &x
    return fallback
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn named_result_closure_defer_storing_local_borrow_rejects() {
    verify(
        r#"
type int_ptr = *int

fn main() {
    f := fn() (out int_ptr) {
        x := 7
        defer fn() {
            out = &x
        }()
        return
    }
    _ = f
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn explicit_non_borrow_return_without_defer_taint_accepts() {
    verify(
        r#"
type int_ptr = *int

fn ok_overwrite(fallback *int) (out int_ptr) {
    x := 42
    out = &x
    defer fn() {
        y := 1
        _ = y
    }()
    return fallback
}
"#,
        None,
    );
}
