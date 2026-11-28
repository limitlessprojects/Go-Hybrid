//! errors_call_buildable_soundness.rs — regression tests for errors member call arity
//! and non-callable symbols (HYB-E1002, Row 13 BUILDABLE soundness, issue #4171).
//!
//! Go standard library contract:
//! - `errors.New(msg)` takes exactly 1 argument.
//! - `errors.Unwrap(err)` takes exactly 1 argument.
//! - `errors.Is(err, target)` takes exactly 2 arguments.
//! - `errors.As(err, target)` takes exactly 2 arguments.
//! - `errors.Join(errs...)` requires at least 1 argument.
//! - `errors.ErrUnsupported` is a package-level variable and must not be callable.
//! - `errors.wrap` has dedicated operand validation (#3572) and is out of scope here.

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
fn test_errors_new_arity_reject() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.new()
    _ = errors.new("a", "b")
}
"#,
    );
}

#[test]
fn test_errors_unwrap_arity_reject() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.unwrap()
}
"#,
    );
}

#[test]
fn test_errors_is_arity_reject() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.is()
}
"#,
    );
}

#[test]
fn test_errors_as_arity_reject() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.as()
}
"#,
    );
}

#[test]
fn test_errors_join_zero_args_reject() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.join()
}
"#,
    );
}

#[test]
fn test_errors_err_unsupported_not_callable() {
    rejects_e1002(
        r#"import "errors"

fn main() {
    _ = errors.err_unsupported()
}
"#,
    );
}

#[test]
fn test_errors_positive_controls_accept() {
    accepts(
        r#"import "errors"

fn step() error {
    return errors.new("boom")
}

fn main() {
    e := errors.new("disk full")
    _ = errors.unwrap(e)
    _ = errors.join(e, e)
    _ = errors.err_unsupported
}
"#,
    );
}
