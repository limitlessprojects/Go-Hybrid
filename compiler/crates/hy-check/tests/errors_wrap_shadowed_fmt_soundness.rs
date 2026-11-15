//! errors_wrap_shadowed_fmt_soundness.rs: regression tests for `errors.wrap` with shadowed `fmt`.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! Go code that compiles under `go build`.
//!
//! `errors.wrap(err, msg)` lowers in `hy-emit` (`expr.rs:1569-1588`) to:
//!     self.use_package("fmt");
//!     Ok(format!("fmt.Errorf(\"%s: %w\", {}, {})", msg, err))
//!
//! When `fmt` is shadowed by an in-scope parameter, local binding, loop variable,
//! closure argument, type parameter, struct, or package-level declaration, downstream Go
//! compilation fails because `fmt.Errorf` resolves to the shadowing entity instead
//! of the standard library `fmt` package.
//!
//! `hy-check` must reject `errors.wrap` calls when `fmt` is shadowed with `HYB-E1002`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_errors_wrap_with_package_const_fmt_rejects() {
    let src = r#"
import "errors"

const fmt = "shadowed"

fn wrap_it(err error) error {
    return errors.wrap(err, "context")
}
"#;
    rejects(src);
}

#[test]
fn test_errors_wrap_with_local_binding_fmt_rejects() {
    let src = r#"
import "errors"

fn wrap_it(err error) error {
    fmt := 42
    return errors.wrap(err, "context")
}
"#;
    rejects(src);
}

#[test]
fn test_errors_wrap_with_param_fmt_rejects() {
    let src = r#"
import "errors"

fn wrap_it(err error, fmt int) error {
    return errors.wrap(err, "context")
}
"#;
    rejects(src);
}

#[test]
fn test_errors_wrap_with_package_struct_fmt_rejects() {
    let src = r#"
import "errors"

struct fmt {
    val int
}

fn wrap_it(err error) error {
    return errors.wrap(err, "context")
}
"#;
    rejects(src);
}

#[test]
fn test_errors_wrap_with_type_param_fmt_rejects() {
    let src = r#"
import "errors"

fn wrap_it[fmt](err error, x fmt) error {
    return errors.wrap(err, "context")
}
"#;
    rejects(src);
}

#[test]
fn test_errors_wrap_with_loop_var_fmt_rejects() {
    let src = r#"
import "errors"

fn wrap_it(err error) error {
    for fmt in 0..5 {
        if fmt == 2 {
            return errors.wrap(err, "context")
        }
    }
    return err
}
"#;
    rejects(src);
}

#[test]
fn test_positive_control_unshadowed_errors_wrap_accepts() {
    let src = r#"
import "errors"

fn wrap_it(err error) error {
    return errors.wrap(err, "context")
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_other_pkg_shadowed_accepts() {
    let src = r#"
import "errors"

const other = 123

fn wrap_it(err error) error {
    return errors.wrap(err, "context")
}
"#;
    accepts(src);
}
