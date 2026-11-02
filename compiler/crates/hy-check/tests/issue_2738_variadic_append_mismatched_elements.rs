//! issue_2738_variadic_append_mismatched_elements.rs — regression tests for Issue #2738:
//! rejection of mismatched slice and string elements in variadic append(a, b...) with HYB-E1002 under Row 13 (BUILDABLE).

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
fn test_append_spread_mismatched_slice_elements_rejected() {
    let cases = [
        ("[]int and []string", "fn test(a []int, b []string) {\n    _ = append(a, b...)\n}\n"),
        ("[]int and []float64", "fn test(a []int, b []float64) {\n    _ = append(a, b...)\n}\n"),
        ("[]float64 and []int", "fn test(a []float64, b []int) {\n    _ = append(a, b...)\n}\n"),
        ("[]bool and []int", "fn test(a []bool, b []int) {\n    _ = append(a, b...)\n}\n"),
        ("[]int and []byte", "fn test(a []int, b []byte) {\n    _ = append(a, b...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_append_spread_string_to_non_byte_slice_rejected() {
    let cases = [
        ("[]int and string", "fn test(a []int, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]int32 and string", "fn test(a []int32, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]rune and string", "fn test(a []rune, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]string and string", "fn test(a []string, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]float64 and string", "fn test(a []float64, s string) {\n    _ = append(a, s...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_append_spread_non_slice_or_string_rejected() {
    let cases = [
        ("[]int and int", "fn test(a []int, n int) {\n    _ = append(a, n...)\n}\n"),
        ("[]int and bool", "fn test(a []int, b bool) {\n    _ = append(a, b...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_append_spread_matching_slices_accepted() {
    let cases = [
        ("[]int and []int", "fn test(a []int, b []int) {\n    _ = append(a, b...)\n}\n"),
        ("[]float64 and []float64", "fn test(a []float64, b []float64) {\n    _ = append(a, b...)\n}\n"),
        ("[]string and []string", "fn test(a []string, b []string) {\n    _ = append(a, b...)\n}\n"),
        ("[]byte and []byte", "fn test(a []byte, b []byte) {\n    _ = append(a, b...)\n}\n"),
        ("[]byte and []uint8", "fn test(a []byte, b []uint8) {\n    _ = append(a, b...)\n}\n"),
        ("[]uint8 and []byte", "fn test(a []uint8, b []byte) {\n    _ = append(a, b...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_append_spread_string_to_byte_slice_accepted() {
    let cases = [
        ("[]byte from string var", "fn test(a []byte, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]uint8 from string var", "fn test(a []uint8, s string) {\n    _ = append(a, s...)\n}\n"),
        ("[]byte from string literal", "fn test(a []byte) {\n    _ = append(a, \"hello\"...)\n}\n"),
        ("[]uint8 from string literal", "fn test(a []uint8) {\n    _ = append(a, \"hello\"...)\n}\n"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {}, got: {:?}",
            desc,
            r.diagnostics
        );
    }
}
