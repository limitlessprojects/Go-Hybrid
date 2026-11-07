//! Regression tests for `Stmt::Assign` inside `if` initializers in `bind_if_init_for_recv`
//! (`HYB-E2008`, Row 13 BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
fn test_closure_if_init_assign_send_only_channel_rejects_e2008() {
    let src = r#"
fn test(ro <-chan int, so chan<- int) {
    f := fn() {
        var ch any = ro
        if ch = so; (<-ch) > 0 {
        }
    }
    f()
}
fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E2008".to_string()),
        "expected HYB-E2008, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_closure_if_init_assign_recv_only_channel_accepts() {
    let src = r#"
fn test(ro <-chan int, so chan<- int) {
    f := fn() {
        var ch any = so
        if ch = ro; (<-ch) > 0 {
        }
    }
    f()
}
fn main() {}
"#;
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "got: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}