//! pointer_conversion_soundness.rs — tests for HYB-E1002 rejection
//! of pointer conversions across incompatible base types,
//! while preserving conversions between identical underlying pointer base types.

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
fn pointer_conversion_int_to_float_rejected() {
    let src = r#"
type float_ptr = *float64

fn test_conv() {
    var i int = 1
    var pi *int = &i
    var pf float_ptr
    pf = float_ptr(pi)
    _ = pf
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for float_ptr(pi), got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `pi` (type `*int`) to type `float_ptr`")));
}

#[test]
fn pointer_conversion_int_to_string_rejected() {
    let src = r#"
type string_ptr = *string

fn test_conv() {
    var i int = 1
    var pi *int = &i
    var ps string_ptr
    ps = string_ptr(pi)
    _ = ps
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for string_ptr(pi), got {:?}",
        codes
    );
}

#[test]
fn pointer_conversion_identical_base_types_accepted() {
    let src = r#"
type int_ptr = *int

fn test_conv() {
    var i int = 1
    var pi *int = &i
    var p2 int_ptr
    p2 = int_ptr(pi)
    _ = p2
}
"#;
    let r = check(src);
    let errs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == hy_diag::Severity::Error).collect();
    assert!(
        errs.is_empty(),
        "pointer conversion with identical base type must be accepted, got: {:?}",
        errs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
