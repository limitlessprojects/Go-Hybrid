//! Regression tests for Issue #4223 (Row 13 BUILDABLE, HYB-E1002):
//! 1. Associated functions (`fn Type.name(...)`) are package-level functions
//!    lowered to `Type_name` with no receiver; calling one on an instance
//!    (`b := box.new(1); c := b.new(2)`) must reject with HYB-E1002 because
//!    `b` has type `box` and `new` is not a field or method on `box`.
//! 2. Calling an associated function on its type (`box.new("s")`, `box.new()`,
//!    `box.new(1, 2)`) must validate argument count and parameter types
//!    and reject mismatches with HYB-E1002.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let has_e1002 = r
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002");
    assert!(
        has_e1002,
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let errs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errs.is_empty(),
        "expected zero errors for:\n{}\ngot: {:?}",
        src,
        errs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_associated_fn_called_on_instance_rejects_e1002() {
    // Issue #4223 F1: `b := box.new(1)` binds `b` to `box`; calling `b.new(2)`
    // invokes an associated function through an instance and fails `go build`
    // (`b.new undefined (type box has no field or method new)`).
    rejects_e1002(
        r#"struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn main() {
    b := box.new(1)
    c := b.new(2)
    _ = c
}
"#,
    );
}

#[test]
fn test_associated_fn_argument_type_mismatch_rejects_e1002() {
    // Issue #4223 F2: `box.new("s")` passes a `string` to an `int` parameter.
    rejects_e1002(
        r#"struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn main() {
    c := box.new("invalid_string_arg")
    _ = c
}
"#,
    );
}

#[test]
fn test_associated_fn_arity_mismatch_rejects_e1002() {
    // Too few and too many arguments to `box.new(v int)` must reject with HYB-E1002.
    rejects_e1002(
        r#"struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn main() {
    c := box.new()
    _ = c
}
"#,
    );
    rejects_e1002(
        r#"struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn main() {
    c := box.new(1, 2)
    _ = c
}
"#,
    );
}

#[test]
fn test_associated_fn_valid_call_and_member_access_accepts() {
    accepts(
        r#"struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn main() {
    b := box.new(42)
    _ = b.v
}
"#,
    );
}
