//! issue_2548_slice_map_literal_int_var_soundness.rs — regression tests for Issue #2548:
//! verification that non-constant integer variables in float64 slice and map literals
//! are properly rejected with HYB-E1002 while untyped constants remain accepted (fixes #2548).

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
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20
    s1 := []float64{1, 2.5, file_k, local_k}
    m1 := map[float64]string{1: \"val\", file_k: \"v2\", local_k: \"v3\"}
    m2 := map[string]float64{\"k1\": 1, \"k2\": file_k, \"k3\": local_k}
    s_any := []any{1, \"text\", 3.14}
    m_any := map[string]any{\"a\": 1, \"b\": \"text\"}
    _ = s1
    _ = m1
    _ = m2
    _ = s_any
    _ = m_any
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped constants and any literals, got diags: {:?}",
        r.diagnostics
    );
}
