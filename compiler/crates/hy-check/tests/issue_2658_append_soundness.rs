//! issue_2658_append_soundness.rs — tests for HYB-E1002 rejection when
//! append has mismatched spread slices or typed variables.

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
fn append_spread_mismatched_elem_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1}
    ys := []string{\"hello\"}
    _ = append(xs, ys...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_float_int_rejects_e1002() {
    let src = "\
fn main() {
    xs := []float64{1.0}
    ys := []int{2, 3}
    _ = append(xs, ys...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_int_float_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1}
    ys := []float64{2.0, 3.0}
    _ = append(xs, ys...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_string_to_non_byte_slice_rejects_e1002() {
    let src = "\
fn main() {
    xs := []string{\"hello\"}
    _ = append(xs, \"world\"...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_non_slice_non_string_rejects() {
    let src = "\
fn main() {
    xs := []int{1}
    x := 10
    _ = append(xs, x...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string())
            || error_codes(&r).contains(&"HYB-E2005".to_string()),
        "expected HYB-E1002 or HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_typed_int_var_to_float_slice_rejects_e1002() {
    let src = "\
fn main() {
    floats := []float64{1.0}
    var i int = 42
    _ = append(floats, i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_matching_slices_accepted() {
    let src = "\
fn main() {
    xs := []int{1}
    ys := []int{2, 3}
    _ = append(xs, ys...)

    fs := []float64{1.0}
    gs := []float64{2.0, 3.0}
    _ = append(fs, gs...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_spread_string_to_byte_slice_accepted() {
    let src = "\
fn main() {
    var bs []byte
    _ = append(bs, \"hello\"...)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_untyped_int_const_to_float_slice_accepted() {
    let src = "\
fn main() {
    floats := []float64{1.0}
    _ = append(floats, 42)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
