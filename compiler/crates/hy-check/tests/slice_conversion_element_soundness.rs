//! slice_conversion_element_soundness.rs — regression tests for Issue #2725:
//! verification that slice and array type conversions with mismatched element types
//! are rejected with HYB-E1002 under Row 13 (BUILDABLE), adhering to Go spec §Conversions.

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
fn mismatched_slice_to_slice_conversions_rejected() {
    // 1. []string from []int
    let src1 = "fn main() {\n    var s []int\n    _ = []string(s)\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. []int from []float64
    let src2 = "fn main() {\n    var f []float64\n    _ = []int(f)\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. []bool from []int
    let src3 = "fn main() {\n    var s []int\n    _ = []bool(s)\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));

    // 4. []int from []string
    let src4 = "fn main() {\n    var s []string\n    _ = []int(s)\n}";
    let r4 = check(src4);
    assert_eq!(r4.verdict, Verdict::Reject);
    assert!(error_codes(&r4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn mismatched_string_to_slice_conversions_rejected() {
    // 1. []float64 from string
    let src1 = "fn main() {\n    var s string = \"test\"\n    _ = []float64(s)\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. []bool from string
    let src2 = "fn main() {\n    var s string = \"test\"\n    _ = []bool(s)\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn mismatched_array_and_slice_conversions_rejected() {
    // 1. [3]string from []int via named type alias
    let src1 = "type str_arr = [3]string\nfn main() {\n    var s []int\n    _ = str_arr(s)\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. []string from [3]int
    let src2 = "fn main() {\n    var a [3]int\n    _ = []string(a)\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_conversions_stay_accepted() {
    let src = r#"
type int_arr = [3]int

fn main() {
    // 1. Legal string to []byte or []rune
    var s string = "hello"
    _ = []byte(s)
    _ = []rune(s)

    // 2. Legal []byte / []rune to string
    var b []byte
    _ = string(b)
    var r []rune
    _ = string(r)

    // 3. Same element slice conversion
    var ints []int
    _ = []int(ints)

    // 4. Same element array conversion from slice
    _ = int_arr(ints)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
}
