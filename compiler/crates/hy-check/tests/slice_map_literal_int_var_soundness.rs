//! slice_map_literal_int_var_soundness.rs — regression tests for Issue #2548:
//! verification that non-constant integer variables in float64 slice and map literals
//! are properly rejected with HYB-E1002 while untyped constants remain accepted.

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
fn int_var_in_float_slice_and_map_literals_rejected_with_e1002() {
    // 1. int variable in float64 slice literal
    let src_slice = "fn main() {\n    var i int = 1\n    s := []float64{i}\n    _ = s\n}\n";
    let r_slice = check(src_slice);
    assert_eq!(r_slice.verdict, Verdict::Reject);
    assert!(error_codes(&r_slice).contains(&"HYB-E1002".to_string()));

    // 2. int variable as float64 map key
    let src_map_key = "fn main() {\n    var i int = 1\n    m := map[float64]string{i: \"val\"}\n    _ = m\n}\n";
    let r_map_key = check(src_map_key);
    assert_eq!(r_map_key.verdict, Verdict::Reject);
    assert!(error_codes(&r_map_key).contains(&"HYB-E1002".to_string()));

    // 3. int variable as float64 map value
    let src_map_val = "fn main() {\n    var i int = 1\n    m := map[string]float64{\"key\": i}\n    _ = m\n}\n";
    let r_map_val = check(src_map_val);
    assert_eq!(r_map_val.verdict, Verdict::Reject);
    assert!(error_codes(&r_map_val).contains(&"HYB-E1002".to_string()));
}

#[test]
fn untyped_constants_and_any_literals_stay_accepted() {
    // 1. untyped int constant in float64 slice literal
    let src_slice_const = "fn main() {\n    s := []float64{1, 2.5}\n    _ = s\n}\n";
    let r_slice_const = check(src_slice_const);
    assert_eq!(r_slice_const.verdict, Verdict::Accept);

    // 2. untyped int constant in float64 map key and value
    let src_map_const = "fn main() {\n    m1 := map[float64]string{1: \"val\"}\n    _ = m1\n    m2 := map[string]float64{\"k\": 1}\n    _ = m2\n}\n";
    let r_map_const = check(src_map_const);
    assert_eq!(r_map_const.verdict, Verdict::Accept);

    // 3. any interface literal elements
    let src_any = "fn main() {\n    s := []any{1, \"text\", 3.14}\n    _ = s\n    m := map[string]any{\"a\": 1, \"b\": \"text\"}\n    _ = m\n}\n";
    let r_any = check(src_any);
    assert_eq!(r_any.verdict, Verdict::Accept);
}
