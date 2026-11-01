//! copy_builtin_soundness.rs — regression tests for copy(dst, src) element type checking:
//! verifies that copy between slices of different element types (including []float64 and []int in both directions)
//! is properly rejected with HYB-E1002, while slices of identical element types remain accepted (fixes #2655).

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
fn copy_float_and_int_slices_rejected_with_e1002() {
    // 1. copy([]float64, []int) — previously escaped due to asymmetric kinds_conflict
    let src1 = "\
fn main() {
    var dst []float64
    var src []int
    copy(dst, src)
}
";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject, "expected reject for copy([]float64, []int)");
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. copy([]int, []float64)
    let src2 = "\
fn main() {
    var dst []int
    var src []float64
    copy(dst, src)
}
";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject, "expected reject for copy([]int, []float64)");
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_different_element_types_rejected_with_e1002() {
    let src = "\
fn main() {
    var dst []float64
    var src []string
    copy(dst, src)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_identical_element_types_stay_accepted() {
    let src = "\
fn main() {
    var d1 []float64
    var s1 []float64
    copy(d1, s1)

    var d2 []int
    var s2 []int
    copy(d2, s2)

    var d3 []string
    var s3 []string
    copy(d3, s3)
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for copy with identical element types, got diags: {:?}",
        r.diagnostics
    );
}
