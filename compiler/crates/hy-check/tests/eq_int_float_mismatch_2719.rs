//! eq_int_float_mismatch_2719.rs — issue #2719 (Row 13 BUILDABLE).
//!
//! A typed integer and a typed float are different types, so Go refuses to
//! compare them with `==` / `!=` (`invalid operation: a == b (mismatched types
//! int and float64)`). The arithmetic and ordered-comparison operators already
//! reject the mixed pair; the equality arm exempted *all* numeric pairs, so
//! `a == b` / `a != b` slipped through with 0 diagnostics. An untyped numeric
//! constant adapts (`int_var == 1.0`, `float_var == 1`, `1 == 1.0` all build),
//! so those must stay accepted.

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
fn typed_int_vs_typed_float_equality_rejects_e1002() {
    let src = "\
fn main() {
    var a int = 1
    var b float64 = 2.0
    _ = a == b
    _ = a != b
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn typed_float_vs_typed_int_equality_rejects_e1002() {
    let src = "\
fn main() {
    var b float64 = 2.0
    var a int32 = 1
    _ = b == a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_constant_equality_stays_accepted() {
    let src = "\
fn main() {
    var a int = 1
    var f float64 = 2.0
    _ = f == 1
    _ = f != 1
    _ = 1 == 1.0
    _ = a == 1
    _ = a != 1
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn same_type_equality_stays_accepted() {
    let src = "\
fn main() {
    var a int = 1
    var b int = 2
    _ = a == b
    var x float64 = 1.0
    var y float64 = 2.0
    _ = x != y
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
