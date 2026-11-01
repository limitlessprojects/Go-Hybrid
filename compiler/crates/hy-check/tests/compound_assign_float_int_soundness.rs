//! compound_assign_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables in compound assignments to float64 targets,
//! while preserving untyped integer constants adaptation.

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
fn compound_assign_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_compound_assign() {
    var f float64 = 1.0
    var i int = 1
    f += i
    f -= i
    f *= i
    f /= i
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
}

#[test]
fn compound_assign_untyped_int_constants_stay_accepted() {
    let src = r#"
const k = 2

fn test_compound_assign_const() {
    var f float64 = 1.0
    f += 1
    f -= k
    f *= 3
    f /= k
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped constants should be legal in compound assignment to float64: {:?}",
        res.diagnostics
    );
}
