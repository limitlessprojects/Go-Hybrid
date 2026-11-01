//! assign_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables assigned to float64 targets (single assignment,
//! parallel assignment, struct field assignment, variable declaration initialization,
//! and struct literals), while preserving untyped integer constants adaptation.

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn single_assign_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_single_assign() {
    var f float64 = 1.0
    var i int = 2
    f = i
    _ = f
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for f = i, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `int` to `f` (type `float64`)")));
}

#[test]
fn parallel_assign_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_parallel_assign() {
    var f float64 = 1.0
    var s string = "a"
    var i int = 2
    f, s = i, "b"
    _ = f
    _ = s
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for parallel assign f, s = i, \"b\", got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `int` to `f` (type `float64`) in parallel assignment")));
}

#[test]
fn struct_field_assign_typed_int_to_float_is_rejected() {
    let src = r#"
struct point {
    x float64
}

fn test_field_assign() {
    var p point
    var i int = 2
    p.x = i
    _ = p
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for p.x = i, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `int` to `p.x` (type `float64`)")));
}

#[test]
fn var_decl_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_var_decl() {
    var i int = 2
    var f float64 = i
    _ = f
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for var f float64 = i, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot initialize `f` (type `float64`) with a value of type `int`")));
}

#[test]
fn struct_literal_typed_int_to_float_is_rejected() {
    let src = r#"
struct point {
    x float64
}

fn test_struct_lit() {
    var i int = 2
    var p point = point{ x: i }
    _ = p
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for point{{ x: i }}, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use a value of type `int` for field `x` of `point`")));
}

#[test]
fn untyped_int_constants_to_float_stay_accepted() {
    let src = r#"
const max_factor = 10

struct point {
    x float64
}

fn test_constants() {
    var f float64 = 1
    f = 2
    f = max_factor
    f, _ = 3, "ok"
    var p point = point{ x: 4 }
    p.x = 5
    p.x = max_factor
    _ = f
    _ = p
}
"#;
    let r = check(src);
    let errs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == hy_diag::Severity::Error).collect();
    assert!(
        errs.is_empty(),
        "untyped int constants assigned to float64 must remain accepted, got: {:?}",
        errs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
