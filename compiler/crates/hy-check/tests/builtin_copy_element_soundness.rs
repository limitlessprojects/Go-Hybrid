//! builtin_copy_element_soundness.rs — regression tests for Issue #2735:
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
fn mismatched_slice_elements_rejected() {
    let cases = [
        ("[]float64 and []int", "fn main() {\n    var dst []float64\n    var src []int\n    _ = copy(dst, src)\n}"),
        ("[]int and []float64", "fn main() {\n    var dst []int\n    var src []float64\n    _ = copy(dst, src)\n}"),
        ("[]int and []string", "fn main() {\n    var dst []int\n    var src []string\n    _ = copy(dst, src)\n}"),
        ("[]bool and []int", "fn main() {\n    var dst []bool\n    var src []int\n    _ = copy(dst, src)\n}"),
        ("[]int and []byte", "fn main() {\n    var dst []int\n    var src []byte\n    _ = copy(dst, src)\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "missing HYB-E1002 for {}", desc);
    }
}

#[test]
fn string_to_non_byte_slice_rejected() {
    let cases = [
        ("[]int from string", "fn main() {\n    var dst []int\n    _ = copy(dst, \"hello\")\n}"),
        ("[]int32 from string", "fn main() {\n    var dst []int32\n    _ = copy(dst, \"hello\")\n}"),
        ("[]rune from string", "fn main() {\n    var dst []rune\n    _ = copy(dst, \"hello\")\n}"),
        ("[]float64 from string", "fn main() {\n    var dst []float64\n    _ = copy(dst, \"hello\")\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "missing HYB-E1002 for {}", desc);
    }
}

#[test]
fn valid_copy_operations_stay_accepted() {
    let src = r#"
fn main() {
    // 1. Identical element types
    var d1 []int
    var s1 []int
    _ = copy(d1, s1)

    var d2 []float64
    var s2 []float64
    _ = copy(d2, s2)

    // 2. Valid byte slice from string
    var d3 []byte
    _ = copy(d3, "hello")

    var d4 []uint8
    _ = copy(d4, "world")
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
}
