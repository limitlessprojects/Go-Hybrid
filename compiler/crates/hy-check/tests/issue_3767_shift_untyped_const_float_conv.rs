//! issue_3767_shift_untyped_const_float_conv.rs — regression tests for Issue #3767:
//! shift of untyped constant inside float/complex type conversion (`float64(1 << n)`)
//! rejected under Row 13 (BUILDABLE) with HYB-E1002.

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
fn float64_untyped_shift_rejects_e1002() {
    let src = "fn compute(n int) float64 {\n    return float64(1 << n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn float32_untyped_shift_rejects_e1002() {
    let src = "fn compute(n int) float32 {\n    return float32(1 << n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn complex128_untyped_shift_rejects_e1002() {
    let src = "fn compute(n int) complex128 {\n    return complex128(1 << n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn complex64_untyped_shift_rejects_e1002() {
    let src = "fn compute(n int) complex64 {\n    return complex64(1 << n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn float64_parenthesized_untyped_shift_rejects_e1002() {
    let src = "fn compute(n int) float64 {\n    return float64((1 << n))\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn float64_right_shift_untyped_rejects_e1002() {
    let src = "fn compute(n int) float64 {\n    return float64(100 >> n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn float64_constant_shift_accepts() {
    let src = "fn compute() float64 {\n    return float64(1 << 3)\n}\nfn main() {\n    _ = compute()\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}

#[test]
fn float64_typed_int_variable_shift_accepts() {
    let src = "fn compute(x int, n int) float64 {\n    return float64(x << n)\n}\nfn main() {\n    _ = compute(1, 3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}

#[test]
fn int_untyped_shift_accepts() {
    let src = "fn compute(n int) int {\n    return int(1 << n)\n}\nfn main() {\n    _ = compute(3)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}
