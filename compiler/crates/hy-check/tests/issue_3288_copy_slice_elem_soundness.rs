//! issue_3288_copy_slice_elem_soundness.rs — tests for HYB-E1002 rejection when
//! copy arguments have mismatched slice element types (scalars, arrays, nested slices, or non-byte string copies).

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
fn copy_mismatched_int_scalars_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int32
    var src []int64
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
fn copy_signed_and_unsigned_int_scalars_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int
    var src []uint
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
fn copy_mismatched_float_scalars_rejects_e1002() {
    let src = "\
fn main() {
    var dst []float32
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
fn copy_mismatched_array_dimensions_rejects_e1002() {
    let src = "\
fn main() {
    var dst [][2]int
    var src [][3]int
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
fn copy_mismatched_nested_slice_scalars_rejects_e1002() {
    let src = "\
fn main() {
    var dst [][]int32
    var src [][]int64
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
fn copy_string_into_int64_slice_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int64
    copy(dst, \"hello\")
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
fn copy_string_into_int_slice_rejects_e1002() {
    let src = "\
fn main() {
    var dst []int
    copy(dst, \"hello\")
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
fn copy_matching_int_slices_accepted() {
    let src = "\
fn main() {
    var dst []int64
    var src []int64
    _ = copy(dst, src)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn copy_string_into_byte_slice_accepted() {
    let src = "\
fn main() {
    var dst []byte
    _ = copy(dst, \"hello\")
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
