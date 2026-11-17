//! context_call_buildable_soundness.rs — regression tests for context package member call arity and non-callable symbols
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library contract (context package):
//! - Types/interfaces (`Context`, `CancelFunc`, `CancelCauseFunc`) and error vars (`Canceled`, `DeadlineExceeded`) are not functions.
//! - Functions have fixed parameter arities (Background, TODO: 0; WithCancel, WithCancelCause, Cause: 1; WithTimeout, WithDeadline, AfterFunc: 2; WithValue, WithTimeoutCause, WithDeadlineCause: 3).
//! - Context functions do not accept variadic spread (`...`).

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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected zero errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_context_non_callable_symbols_rejected() {
    // Calling interface/type or error variables as functions must emit HYB-E1002
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    _ = context.context()
}"#,
    );
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    _ = context.cancel_func()
}"#,
    );
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    _ = context.canceled()
}"#,
    );
}

#[test]
fn test_context_arity_mismatches_rejected() {
    // background takes 0 args
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    _ = context.background(1)
}"#,
    );
    // with_cancel takes 1 arg
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    _ = context.with_cancel()
}"#,
    );
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    let ctx = context.background()
    _ = context.with_cancel(ctx, 1)
}"#,
    );
    // with_timeout takes 2 args
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    let ctx = context.background()
    _ = context.with_timeout(ctx)
}"#,
    );
    // with_value takes 3 args
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    let ctx = context.background()
    _ = context.with_value(ctx, "k")
}"#,
    );
}

#[test]
fn test_context_variadic_spread_rejected() {
    rejects_e1002(
        r#"package main
import "context"
fn main() {
    let s = []int{1, 2}
    _ = context.background(s...)
}"#,
    );
}

#[test]
fn test_context_valid_calls_accepted() {
    accepts(
        r#"package main
import "context"
fn main() {
    ctx := context.background()
    _ = context.todo()
    c, cancel := context.with_cancel(ctx)
    _ = context.with_value(c, "key", "val")
    _ = cancel
}"#,
    );
}
