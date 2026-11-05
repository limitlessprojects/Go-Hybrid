//! append_nil_soundness.rs — regression tests for Issue #2745:
//! verification that passing untyped nil as first argument to append is rejected with HYB-E2005,
//! and appending nil to slices with non-nilable element types (int, string, bool, etc.) is rejected with HYB-E1002
//! under Row 13 (BUILDABLE), adhering to Go spec §Appending to and copying slices and §Assignability.

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
fn append_nil_first_arg_rejects_e2005() {
    let src = "fn main() {\n    _ = append(nil, 1)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E2005".to_string()));
}

#[test]
fn append_nil_to_int_slice_rejects_e1002() {
    let src = "fn test(xs []int) {\n    _ = append(xs, nil)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_nil_to_string_slice_rejects_e1002() {
    let src = "fn test(xs []string) {\n    _ = append(xs, nil)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_nil_to_bool_slice_rejects_e1002() {
    let src = "fn test(xs []bool) {\n    _ = append(xs, nil)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_nil_to_nilable_slice_elements_accepts() {
    let cases = [
        "fn test(xs []*int) {\n    _ = append(xs, nil)\n}\n",
        "fn test(xs [][]int) {\n    _ = append(xs, nil)\n}\n",
        "fn test(xs []map[string]int) {\n    _ = append(xs, nil)\n}\n",
        "fn test(xs []chan int) {\n    _ = append(xs, nil)\n}\n",
        "fn test(xs []any) {\n    _ = append(xs, nil)\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for nilable element append, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}
