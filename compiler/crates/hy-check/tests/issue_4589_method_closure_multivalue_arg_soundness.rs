//! Test suite for issue #4589:
//! Multi-value method and closure calls bypass argument type and scalar-width validation (HYB-E1002, Row 13).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn test_method_multivalue_kind_mismatch_rejected() {
    let src = r#"
struct worker {}

fn (w worker) process(a int, b int) {
    _ = a
    _ = b
}

fn pair() (string, string) {
    return "a", "b"
}

fn main() {
    w := worker{}
    w.process(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_method_multivalue_scalar_width_mismatch_rejected() {
    let src = r#"
struct worker {}

fn (w worker) process(a int64, b int64) {
    _ = a
    _ = b
}

fn pair32() (int32, int32) {
    return 1, 2
}

fn main() {
    w := worker{}
    w.process(pair32())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_method_multivalue_arity_mismatch_rejected() {
    let src = r#"
struct worker {}

fn (w worker) process(a int, b int, c int) {
    _ = a
    _ = b
    _ = c
}

fn pair() (int, int) {
    return 1, 2
}

fn main() {
    w := worker{}
    w.process(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_closure_multivalue_kind_mismatch_rejected() {
    let src = r#"
fn pair() (string, string) {
    return "a", "b"
}

fn main() {
    (fn(a int, b int) {
        _ = a
        _ = b
    })(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_closure_multivalue_scalar_width_mismatch_rejected() {
    let src = r#"
fn pair32() (int32, int32) {
    return 1, 2
}

fn main() {
    (fn(a int64, b int64) {
        _ = a
        _ = b
    })(pair32())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_closure_multivalue_arity_mismatch_rejected() {
    let src = r#"
fn pair() (int, int) {
    return 1, 2
}

fn main() {
    (fn(a int, b int, c int) {
        _ = a
        _ = b
        _ = c
    })(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002" || d.code == "HYB-E2002"),
        "expected HYB-E1002 or HYB-E2002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_method_multivalue_matching_types_accepted() {
    let src = r#"
struct worker {}

fn (w worker) process(a int64, b int64) {
    _ = a
    _ = b
}

fn pair() (int64, int64) {
    return 10, 20
}

fn main() {
    w := worker{}
    w.process(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_closure_multivalue_matching_types_accepted() {
    let src = r#"
fn pair() (string, string) {
    return "hello", "world"
}

fn main() {
    (fn(a string, b string) {
        _ = a
        _ = b
    })(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
