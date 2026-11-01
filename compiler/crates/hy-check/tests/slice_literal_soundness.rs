//! slice_literal_soundness.rs — regression tests for slice composite literals:
//! verifies that initializing float64 slices with typed int variables is rejected with HYB-E1002,
//! while untyped integer constants and []any slices remain cleanly accepted (fixes #2683).

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
fn typed_int_var_in_float_slice_literal_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 42
    s := []float64{i}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []float64 with typed int var, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_multi_element_float_slice_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 42
    s := []float64{1.0, i, 3.14}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for multi-element []float64 with typed int, got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_float_slice_literal_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20

    s1 := []float64{1, 2, 3}
    s2 := []float64{local_k, file_k}
    s3 := []float64{-5, 1 + 2}

    _ = s1
    _ = s2
    _ = s3
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped constants in float slice literals, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn any_slice_literal_with_diverse_elements_accepted() {
    let src = "\
fn main() {
    s := []any{1, \"str\", 3.14, true}
    _ = s
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for diverse elements in []any slice literal, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn mismatched_slice_literal_types_rejected() {
    let src = "\
fn main() {
    s := []string{42}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for string slice with int element, got {:?}",
        r.diagnostics
    );
}
