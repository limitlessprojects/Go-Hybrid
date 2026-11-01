//! issue_2674_var_decl_float_int_soundness.rs

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
fn var_decl_typed_int_to_float_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 42
    var f float64 = i
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn let_typed_int_to_float_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 42
    let f float64 = i
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn struct_lit_typed_int_to_float_rejects_e1002() {
    let src = "\
struct item {
    f float64
}
fn main() {
    var i int = 42
    s := item{f: i}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn assign_typed_int_to_float_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 42
    var f float64
    f = i
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn field_assign_typed_int_to_float_rejects_e1002() {
    let src = "\
struct item {
    f float64
}
fn main() {
    var i int = 42
    var s item
    s.f = i
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn parallel_assign_typed_int_to_float_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 42
    var f1 float64
    var f2 float64
    f1, f2 = 1.0, i
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn var_decl_untyped_int_const_to_float_accepted() {
    let src = "\
const c = 10
fn main() {
    var f float64 = 1
    var f2 float64 = c
    var f3 float64 = 1 + 2
    _ = f
    _ = f2
    _ = f3
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
