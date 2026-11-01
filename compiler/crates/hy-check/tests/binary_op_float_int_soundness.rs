//! binary_op_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of mismatched float64 and int typed variables in binary operations,
//! while preserving untyped integer/float constants adaptation.

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
fn binary_arith_typed_float_and_int_is_rejected() {
    let src = r#"
fn test_bin_arith() {
    var f float64 = 1.0
    var i int = 1
    _ = f + i
    _ = f - i
    _ = f * i
    _ = f / i
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
fn binary_cmp_typed_float_and_int_is_rejected() {
    let src = r#"
fn test_bin_cmp() {
    var f float64 = 1.0
    var i int = 1
    _ = f == i
    _ = f != i
    _ = f < i
    _ = f <= i
    _ = f > i
    _ = f >= i
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
fn binary_untyped_constants_stay_accepted() {
    let src = r#"
const k = 2

fn test_bin_const() {
    var f float64 = 1.0
    var i int = 1
    _ = f + 1
    _ = f - k
    _ = f * 3
    _ = f / k
    _ = f == 1
    _ = f < k
    _ = 1 + f
    _ = k * f
    _ = i + 1
    _ = i == 1
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped constants should be legal in binary operations with float64: {:?}",
        res.diagnostics
    );
}
