//! min_max_builtin_soundness.rs — regression tests for Issue #2542:
//! verification of ordered types (HYB-E2005) and type matching (HYB-E1002) for `min` and `max`.

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
fn min_max_unordered_operands_rejected_with_e2005() {
    // 1. bool operand in min
    let src_bool = "fn main() {\n    _ = min(true, false)\n}\n";
    let r_bool = check(src_bool);
    assert_eq!(r_bool.verdict, Verdict::Reject);
    assert!(error_codes(&r_bool).contains(&"HYB-E2005".to_string()));

    // 2. slice operand in max
    let src_slice = "fn main() {\n    s1 := []int{1}\n    s2 := []int{2}\n    _ = max(s1, s2)\n}\n";
    let r_slice = check(src_slice);
    assert_eq!(r_slice.verdict, Verdict::Reject);
    assert!(error_codes(&r_slice).contains(&"HYB-E2005".to_string()));

    // 3. map operand in min
    let src_map = "fn main() {\n    m1 := map[string]int{}\n    m2 := map[string]int{}\n    _ = min(m1, m2)\n}\n";
    let r_map = check(src_map);
    assert_eq!(r_map.verdict, Verdict::Reject);
    assert!(error_codes(&r_map).contains(&"HYB-E2005".to_string()));
}

#[test]
fn min_max_mismatched_operand_types_rejected_with_e1002() {
    // 1. string and int in min
    let src_str_int = "fn main() {\n    _ = min(\"hello\", 42)\n}\n";
    let r_str_int = check(src_str_int);
    assert_eq!(r_str_int.verdict, Verdict::Reject);
    assert!(error_codes(&r_str_int).contains(&"HYB-E1002".to_string()));

    // 2. float and string in max
    let src_flt_str = "fn main() {\n    _ = max(3.14, \"world\")\n}\n";
    let r_flt_str = check(src_flt_str);
    assert_eq!(r_flt_str.verdict, Verdict::Reject);
    assert!(error_codes(&r_flt_str).contains(&"HYB-E1002".to_string()));
}

#[test]
fn min_max_valid_ordered_operands_accepted() {
    // 1. integers in min
    let src_ints = "fn main() {\n    _ = min(1, 2, 3)\n}\n";
    let r_ints = check(src_ints);
    assert_eq!(r_ints.verdict, Verdict::Accept);

    // 2. strings in max
    let src_strs = "fn main() {\n    _ = max(\"apple\", \"banana\")\n}\n";
    let r_strs = check(src_strs);
    assert_eq!(r_strs.verdict, Verdict::Accept);

    // 3. floats in min
    let src_flts = "fn main() {\n    _ = min(1.5, 2.5)\n}\n";
    let r_flts = check(src_flts);
    assert_eq!(r_flts.verdict, Verdict::Accept);
}
