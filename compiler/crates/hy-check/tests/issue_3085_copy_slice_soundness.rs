//! issue_3085_copy_slice_soundness.rs — tests for HYB-E1002 rejection of
//! slice element scalar mismatches in `copy(dst, src)` and copying string into non-byte integer slices.

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
fn copy_slice_int_and_int64_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]int, 5)\n    src := make([]int64, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_slice_byte_and_rune_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]byte, 5)\n    src := make([]rune, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_slice_float32_and_float64_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]float32, 5)\n    src := make([]float64, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_slice_complex64_and_complex128_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]complex64, 5)\n    src := make([]complex128, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_into_int_slice_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]int, 5)\n    copy(dst, \"hello\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_into_rune_slice_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]rune, 5)\n    copy(dst, \"hello\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_into_int64_slice_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]int64, 5)\n    copy(dst, \"hello\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_slice_params_mismatch_rejects_e1002() {
    let src = "fn copy_items(dst []int, src []int64) {\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_param_mismatch_rejects_e1002() {
    let src = "fn fill(dst []int, s string) {\n    copy(dst, s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_subslice_mismatch_rejects_e1002() {
    let src = "fn main() {\n    dst := make([]int, 10)\n    src := make([]int64, 10)\n    copy(dst[1..], src[2..])\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_string_into_byte_slice_accepted() {
    let src = "fn main() {\n    dst := make([]byte, 5)\n    copy(dst, \"hello\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn copy_string_into_uint8_slice_accepted() {
    let src = "fn main() {\n    dst := make([]uint8, 5)\n    copy(dst, \"hello\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn copy_slice_byte_and_uint8_accepted() {
    let src = "fn main() {\n    dst := make([]byte, 5)\n    src := make([]uint8, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn copy_slice_rune_and_int32_accepted() {
    let src = "fn main() {\n    dst := make([]rune, 5)\n    src := make([]int32, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn copy_slice_identical_types_accepted() {
    let src = "fn main() {\n    dst := make([]int, 5)\n    src := make([]int, 5)\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
