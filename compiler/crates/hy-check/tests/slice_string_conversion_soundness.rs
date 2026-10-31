//! slice_string_conversion_soundness.rs — regression tests for Issue #2546:
//! verification of element kind compatibility in type conversions between string and slice/array.

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
fn string_conversion_from_invalid_slice_or_array_rejected_with_e1002() {
    // 1. cannot convert []float64 to string
    let src_flt = "fn main() {\n    fs := []float64{1.0, 2.0}\n    _ = string(fs)\n}\n";
    let r_flt = check(src_flt);
    assert_eq!(r_flt.verdict, Verdict::Reject);
    assert!(error_codes(&r_flt).contains(&"HYB-E1002".to_string()));

    // 2. cannot convert []string to string
    let src_str = "fn main() {\n    ss := []string{\"a\", \"b\"}\n    _ = string(ss)\n}\n";
    let r_str = check(src_str);
    assert_eq!(r_str.verdict, Verdict::Reject);
    assert!(error_codes(&r_str).contains(&"HYB-E1002".to_string()));

    // 3. cannot convert []bool to string
    let src_bool = "fn main() {\n    bs := []bool{true, false}\n    _ = string(bs)\n}\n";
    let r_bool = check(src_bool);
    assert_eq!(r_bool.verdict, Verdict::Reject);
    assert!(error_codes(&r_bool).contains(&"HYB-E1002".to_string()));

    // 4. cannot convert [N]int array directly to string
    let src_arr = "fn main() {\n    arr := [2]int{65, 66}\n    _ = string(arr)\n}\n";
    let r_arr = check(src_arr);
    assert_eq!(r_arr.verdict, Verdict::Reject);
    assert!(error_codes(&r_arr).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_string_conversions_accepted() {
    // 1. byte/rune slice to string
    let src_bytes = "fn main() {\n    bs := []int{65, 66}\n    _ = string(bs)\n}\n";
    let r_bytes = check(src_bytes);
    assert_eq!(r_bytes.verdict, Verdict::Accept);

    // 2. int code point to string
    let src_cp = "fn main() {\n    _ = string(65)\n}\n";
    let r_cp = check(src_cp);
    assert_eq!(r_cp.verdict, Verdict::Accept);

    // 3. string to string
    let src_id = "fn main() {\n    _ = string(\"hello\")\n}\n";
    let r_id = check(src_id);
    assert_eq!(r_id.verdict, Verdict::Accept);
}
