//! Regression tests for Issue #4604:
//! Defer calls on methods, associated functions, and closures bypass call arity,
//! argument type, and scalar width validation (HYB-E1002, Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("must parse");
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn defer_method_arity_excess_rejected_e1002() {
    let src = r#"
struct service {}

fn (s service) close() {
}

fn main() {
    s := service{}
    defer s.close(1, 2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_method_arity_missing_rejected_e1002() {
    let src = r#"
struct service {}

fn (s service) ping(timeout int) {
    _ = timeout
}

fn main() {
    s := service{}
    defer s.ping()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_method_arg_type_mismatch_rejected_e1002() {
    let src = r#"
struct service {}

fn (s service) ping(timeout int) {
    _ = timeout
}

fn main() {
    s := service{}
    defer s.ping("not an int")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_method_scalar_width_mismatch_rejected_e1002() {
    let src = r#"
struct service {}

fn (s service) ping(timeout int64) {
    _ = timeout
}

fn main() {
    s := service{}
    var x int32 = 5
    defer s.ping(x)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_associated_fn_excess_args_rejected_e1002() {
    let src = r#"
struct service {}

fn service.cleanup() {
}

fn main() {
    defer service.cleanup(42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_associated_fn_missing_args_rejected_e1002() {
    let src = r#"
struct service {}

fn service.setup(tag string, count int) {
    _ = tag
    _ = count
}

fn main() {
    defer service.setup("init")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_associated_fn_type_mismatch_rejected_e1002() {
    let src = r#"
struct service {}

fn service.setup(tag string, count int) {
    _ = tag
    _ = count
}

fn main() {
    defer service.setup(123, "wrong")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_closure_arity_rejected_e1002() {
    let src = r#"
fn main() {
    defer (fn(x int) { _ = x })()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_closure_type_mismatch_rejected_e1002() {
    let src = r#"
fn main() {
    defer (fn(x int) { _ = x })("not an int")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_non_variadic_spread_rejected_e1002() {
    let src = r#"
fn helper(a int, b int) {
    _ = a
    _ = b
}

fn main() {
    xs := [1, 2]
    defer helper(xs...)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn defer_valid_calls_accepted() {
    let src = r#"
struct service {}

fn (s service) close() {
}

fn (s service) ping(timeout int) {
    _ = timeout
}

fn service.cleanup() {
}

fn helper(a int) {
    _ = a
}

fn main() {
    s := service{}
    defer s.close()
    defer s.ping(10)
    defer service.cleanup()
    defer helper(42)
    defer (fn(x int) { _ = x })(100)
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
