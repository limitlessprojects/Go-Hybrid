//! Regression tests for float constant zero divisors in compound assignment (`/=`, `%=`)
//! in `hy-check/src/arith.rs` (`HYB-E6001`, Row 13 BUILDABLE).

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

#[test]
fn test_div_assign_float_literal_zero_rejects_e6001() {
    let src = r#"
fn main() {
    f := 1.5
    f /= 0.0
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6001".to_string()));
}

#[test]
fn test_div_assign_folded_float_const_zero_rejects_e6001() {
    let src = r#"
const ZERO_F = 0.0

fn main() {
    f := 1.5
    f /= ZERO_F
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6001".to_string()));
}

#[test]
fn test_div_assign_nonzero_and_runtime_float_accept() {
    let src = r#"
const TWO_F = 2.0

fn main() {
    f := 1.5
    d := 0.0
    f /= 2.0
    f /= TWO_F
    f /= d
    _ = f
}
"#;
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "got: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}