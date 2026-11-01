//! append_spread_element_soundness.rs — tests for HYB-E1002 rejection of
//! mismatched slice and string elements in variadic append(a, b...).
//! Closes #2738.

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
fn append_spread_mismatched_slice_elements_rejects_e1002() {
    let src = "fn test(a []int, b []string) {\n    _ = append(a, b...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_spread_string_to_non_byte_slice_rejects_e1002() {
    let src = "fn test(a []int, s string) {\n    _ = append(a, s...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_spread_non_slice_or_string_rejects_e1002() {
    let src = "fn test(a []int, n int) {\n    _ = append(a, n...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_spread_matching_slice_accepts() {
    let src = "fn test(a []int, b []int) {\n    _ = append(a, b...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn append_spread_string_to_byte_slice_accepts() {
    let src = "fn test(a []byte, s string) {\n    _ = append(a, s...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
