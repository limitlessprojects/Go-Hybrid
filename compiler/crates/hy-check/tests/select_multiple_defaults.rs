//! select_multiple_defaults.rs - tests for HYB-E1002 rejection of
//! multiple default clauses in a select statement (#2356).
//!
//! Go syntax / semantic rules:
//! A select statement may have at most one default case.
//! A select statement with two or more default arms fails go build with
//! 'multiple defaults' (Row 13 BUILDABLE).

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
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn select_with_two_defaults_rejects_e1002() {
    let src = r#"
fn main() {
    select {
        default: { fmt.println(0) }
        default: { fmt.println(1) }
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn select_with_three_defaults_rejects_e1002() {
    let src = r#"
fn main() {
    select {
        default: { fmt.println(0) }
        default: { fmt.println(1) }
        default: { fmt.println(2) }
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let count = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .count();
    assert_eq!(count, 2, "expected 2 diagnostics for 2 extra defaults");
}

#[test]
fn select_with_single_default_recv_positive_control() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    select {
        v := <-ch: { fmt.println(v) }
        default: { fmt.println(0) }
    }
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "positive control should have 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn select_with_single_default_send_positive_control() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    select {
        ch <- 1: { }
        default: { }
    }
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "positive control should have 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn select_with_default_first_positive_control() {
    let src = r#"
fn main() {
    ch := make(chan int, 1)
    select {
        default: { fmt.println(0) }
        v := <-ch: { fmt.println(v) }
    }
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "positive control should have 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
