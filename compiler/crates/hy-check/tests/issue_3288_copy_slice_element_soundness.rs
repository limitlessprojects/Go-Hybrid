//! Regression test for issue #3288:
//! hyb-check: enforce slice element type equality in built-in copy(dst, src) (HYB-E1002).
//!
//! Under Go Spec §Copy and GUARANTEES.md Row 13 (BUILDABLE), copy(dst, src) requires
//! both arguments to have identical element types T (or dst is []byte/[]uint8 and src is string).
//! hy-check must reject scalar width mismatches, array dimension mismatches, and nested slice
//! scalar mismatches with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet should parse");
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
fn test_copy_numeric_scalar_mismatch_int32_int64_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst []int32
    var src []int64
    copy(dst, src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []int32 vs []int64 copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_numeric_scalar_mismatch_int_uint_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst []int
    var src []uint
    copy(dst, src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []int vs []uint copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_numeric_scalar_mismatch_float32_float64_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst []float32
    var src []float64
    copy(dst, src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []float32 vs []float64 copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_array_dim_mismatch_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var a [][2]int
    var b [][3]int
    copy(a, b)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for [][2]int vs [][3]int copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_array_elem_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var a [][2]int32
    var b [][2]int64
    copy(a, b)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for [][2]int32 vs [][2]int64 copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_nested_slice_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst [][]int32
    var src [][]int64
    copy(dst, src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for [][]int32 vs [][]int64 copy, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_string_to_int_slice_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst []int
    copy(dst, "hello")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []int destination with string source, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_string_to_int32_slice_rejects_e1002() {
    let src = r#"
fn test_mismatch() {
    var dst []int32
    copy(dst, "hello")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []int32 destination with string source, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_copy_matching_numeric_scalars_accepted() {
    let src = r#"
fn test_matching() {
    var dst []int32
    var src []int32
    copy(dst, src)

    var d64 []float64
    var s64 []float64
    copy(d64, s64)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching numeric slices, got {:?}", r.diagnostics);
}

#[test]
fn test_copy_matching_array_slices_accepted() {
    let src = r#"
fn test_matching() {
    var a [][2]int
    var b [][2]int
    copy(a, b)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching array slices, got {:?}", r.diagnostics);
}

#[test]
fn test_copy_matching_nested_slices_accepted() {
    let src = r#"
fn test_matching() {
    var a [][]int32
    var b [][]int32
    copy(a, b)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching nested slices, got {:?}", r.diagnostics);
}

#[test]
fn test_copy_string_to_byte_slice_accepted() {
    let src = r#"
fn test_byte_string_copy() {
    var dst []byte
    copy(dst, "hello")

    var u []uint8
    copy(u, "world")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for byte/uint8 slice with string, got {:?}", r.diagnostics);
}
