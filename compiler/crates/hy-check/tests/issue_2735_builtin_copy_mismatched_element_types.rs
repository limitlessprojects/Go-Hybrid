//! issue_2735_builtin_copy_mismatched_element_types.rs — regression tests for Issue #2735:
//! verification that built-in `copy(dst, src)` rejects mismatched slice element types
//! and string copying into non-byte slice destinations with HYB-E1002 under Row 13 (BUILDABLE),
//! adhering to Go spec §Copy.

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
fn test_mismatched_slice_elements_rejected_with_e1002() {
    let cases = [
        ("[]float64 and []int", "fn main() {\n    var dst []float64\n    var src []int\n    _ = copy(dst, src)\n}"),
        ("[]int and []float64", "fn main() {\n    var dst []int\n    var src []float64\n    _ = copy(dst, src)\n}"),
        ("[]int and []string", "fn main() {\n    var dst []int\n    var src []string\n    _ = copy(dst, src)\n}"),
        ("[]bool and []int", "fn main() {\n    var dst []bool\n    var src []int\n    _ = copy(dst, src)\n}"),
        ("[]int and []byte", "fn main() {\n    var dst []int\n    var src []byte\n    _ = copy(dst, src)\n}"),
        ("[]int32 and []int64", "fn main() {\n    var dst []int32\n    var src []int64\n    _ = copy(dst, src)\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "missing HYB-E1002 for {}, diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_string_to_non_byte_slice_rejected_with_e1002() {
    let cases = [
        ("[]int from string", "fn main() {\n    var dst []int\n    _ = copy(dst, \"hello\")\n}"),
        ("[]int32 from string", "fn main() {\n    var dst []int32\n    _ = copy(dst, \"hello\")\n}"),
        ("[]rune from string", "fn main() {\n    var dst []rune\n    _ = copy(dst, \"hello\")\n}"),
        ("[]float64 from string", "fn main() {\n    var dst []float64\n    _ = copy(dst, \"hello\")\n}"),
        ("[]string from string", "fn main() {\n    var dst []string\n    _ = copy(dst, \"hello\")\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "missing HYB-E1002 for {}, diagnostics: {:?}",
            desc,
            r.diagnostics
        );
    }
}

#[test]
fn test_valid_copy_operations_accepted() {
    let src = r#"
fn main() {
    // 1. Identical element types
    var d1 []int
    var s1 []int
    _ = copy(d1, s1)

    var d2 []float64
    var s2 []float64
    _ = copy(d2, s2)

    var d3 []string
    var s3 []string
    _ = copy(d3, s3)

    // 2. Canonical byte slice alias compatibility
    var d4 []byte
    var s4 []byte
    _ = copy(d4, s4)

    var d5 []byte
    var s5 []uint8
    _ = copy(d5, s5)

    var d6 []uint8
    var s6 []byte
    _ = copy(d6, s6)

    // 3. Valid byte slice from string
    var d7 []byte
    _ = copy(d7, "hello")

    var d8 []uint8
    _ = copy(d8, "world")
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
}
