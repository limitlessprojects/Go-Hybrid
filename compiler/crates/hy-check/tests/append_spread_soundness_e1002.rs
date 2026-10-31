//! append_spread_soundness_e1002.rs — tests for HYB-E1002 rejection of
//! mismatched element types in variadic slice append (`append(s, t...)`).
//! Issue #2491: SPEC-LANGUAGE §3.1 & GUARANTEES.md Row 13 (BUILDABLE).

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
fn append_spread_with_mismatched_slice_elements_rejects_e1002() {
    let cases = [
        ("[]int and []string", "fn main() {\n    a := []int{1, 2}\n    b := []string{\"x\", \"y\"}\n    _ = append(a, b...)\n}\n"),
        ("[]int and []float64", "fn main() {\n    a := []int{1, 2}\n    b := []float64{1.5, 2.5}\n    _ = append(a, b...)\n}\n"),
        ("[]int and []bool", "fn main() {\n    a := []int{1, 2}\n    b := []bool{true, false}\n    _ = append(a, b...)\n}\n"),
        ("[]string and []int", "fn main() {\n    a := []string{\"a\"}\n    b := []int{1, 2}\n    _ = append(a, b...)\n}\n"),
        ("non-byte slice and string spread", "fn main() {\n    a := []float64{1.0}\n    _ = append(a, \"hello\"...)\n}\n"),
    ];

    for (label, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {label}");
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for {label}, got: {:?}", r.diagnostics);
    }
}

#[test]
fn valid_append_spread_and_single_elements_accept() {
    let src = r#"
fn main() {
    // Matching slice element types
    a := []int{1, 2}
    b := []int{3, 4}
    _ = append(a, b...)

    // Valid string append to []byte
    bs := []byte("init")
    _ = append(bs, "extra"...)

    // Valid single element appends
    _ = append(a, 10, 20)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
