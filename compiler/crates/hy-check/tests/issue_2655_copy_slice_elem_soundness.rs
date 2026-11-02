//! issue_2655_copy_slice_elem_soundness.rs — tests for HYB-E1002 rejection when
//! copy arguments have mismatched slice element types (such as []float64 and []int).

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
fn copy_float_dst_int_src_rejects_e1002() {
    let src = "\
fn main() {
    var dst []float64
    var src []int
    copy(dst, src)
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
fn copy_int_dst_float_src_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int
    var src []float64
    copy(dst, src)
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
fn copy_any_dst_int_src_rejects_e1002() {
    let src = "\
fn main() {
    var dst []any
    var src []int
    copy(dst, src)
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
fn copy_int_dst_any_src_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int
    var src []any
    copy(dst, src)
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
fn copy_matching_float_slices_accepted() {
    let src = "\
fn main() {
    var dst []float64
    var src []float64
    _ = copy(dst, src)
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
fn copy_matching_int_slices_accepted() {
    let src = "\
fn main() {
    var dst []int
    var src []int
    _ = copy(dst, src)
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
