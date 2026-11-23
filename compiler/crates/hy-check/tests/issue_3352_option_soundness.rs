//! issue_3352_option_soundness.rs — Row 13 BUILDABLE soundness.
//!
//! Regression tests for #3352 / HYB-E1002: option[T] generic struct instantiations
//! and some(v) constructor calls must validate payload scalar widths and array
//! lengths across declarations, assignments, equality, call arguments, and returns.

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
fn option_assignment_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var o1 option[int32]
    var o2 option[int64]
    o1 = o2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_equality_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var o1 option[int32]
    var o2 option[int64]
    _ = o1 == o2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_ne_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var o1 option[int32]
    var o2 option[int64]
    _ = o1 != o2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_decl_init_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var n int64 = 1
    var o option[int32] = some(n)
    _ = o
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_fn_arg_scalar_mismatch_rejected() {
    let r = check(
        r#"fn f(op option[int32]) {
    _ = op
}

fn main() {
    var n int64 = 1
    f(some(n))
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_return_scalar_mismatch_rejected() {
    let r = check(
        r#"fn ret() option[int32] {
    var n int64 = 1
    return some(n)
}

fn main() {
    _ = ret()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_sized_array_length_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var a4 [4]int
    var oa option[[3]int] = some(a4)
    _ = oa
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_inferred_binding_assignment_rejected() {
    let r = check(
        r#"fn main() {
    var n int64 = 1
    o := some(n)
    var target option[int32]
    target = o
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_matching_types_accepted() {
    let r = check(
        r#"fn main() {
    var n int32 = 1
    var o option[int32] = some(n)
    var o2 option[int32]
    o2 = o
    _ = o == o2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}
