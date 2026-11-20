//! Regression tests for Issue #4605:
//! Task spawn statements on associated functions (Type.fn) bypass call arity,
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
fn task_associated_fn_missing_args_rejected_e1002() {
    let src = r#"
struct worker {}

fn worker.run(id int, active bool) {
    _ = id
    _ = active
}

fn main() {
    task worker.run()
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
fn task_associated_fn_excess_args_rejected_e1002() {
    let src = r#"
struct worker {}

fn worker.run(id int, active bool) {
    _ = id
    _ = active
}

fn main() {
    task worker.run(1, true, 42)
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
fn task_associated_fn_arg_type_mismatch_rejected_e1002() {
    let src = r#"
struct worker {}

fn worker.run(id int, active bool) {
    _ = id
    _ = active
}

fn main() {
    task worker.run("not an int", 42)
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
fn task_associated_fn_scalar_width_mismatch_rejected_e1002() {
    let src = r#"
struct worker {}

fn worker.run(id int64, active bool) {
    _ = id
    _ = active
}

fn main() {
    var x int32 = 10
    task worker.run(x, true)
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
fn task_associated_fn_non_variadic_spread_rejected_e1002() {
    let src = r#"
struct worker {}

fn worker.run(id int, active bool) {
    _ = id
    _ = active
}

fn main() {
    xs := [1, 2]
    task worker.run(xs...)
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
fn task_closure_arity_rejected_e1002() {
    let src = r#"
fn main() {
    task (fn(a int) { _ = a })()
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
fn task_closure_type_mismatch_rejected_e1002() {
    let src = r#"
fn main() {
    task (fn(a int) { _ = a })("wrong")
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
fn task_closure_scalar_width_mismatch_rejected_e1002() {
    let src = r#"
fn main() {
    var x int32 = 10
    task (fn(a int64) { _ = a })(x)
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
fn task_valid_spawns_accepted() {
    let src = r#"
struct worker {}

fn worker.run(id int, active bool) {
    _ = id
    _ = active
}

fn helper(a int) {
    _ = a
}

fn main() {
    task helper(100)
    managed("safe worker spawn", "other") {
        task worker.run(1, true)
    }
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
