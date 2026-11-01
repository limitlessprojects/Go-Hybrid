//! append_soundness.rs — regression tests for builtin `append` type checks:
//! verifies that variadic spread with mismatched slice/string types and appending
//! typed int variables into float slices are rejected with HYB-E1002,
//! while untyped integer constants and matching slice spreads remain accepted (fixes #2658).

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
fn typed_int_var_appended_to_float_slice_rejected_with_e1002() {
    let src = "\
fn main() {
    xs := []float64{1.0}
    var i int = 42
    ys := append(xs, i)
    _ = ys
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
fn mismatched_slice_types_in_spread_append_rejected_with_e1002() {
    // 1. []int spread into []float64
    let src1 = "\
fn main() {
    xs := []float64{1.0}
    ys := []int{2, 3}
    zs := append(xs, ys...)
    _ = zs
}
";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(
        error_codes(&r1).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []int into []float64 spread, got {:?}",
        r1.diagnostics
    );

    // 2. []string spread into []int
    let src2 = "\
fn main() {
    xs := []int{1}
    ys := []string{\"bad\"}
    zs := append(xs, ys...)
    _ = zs
}
";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(
        error_codes(&r2).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for []string into []int spread, got {:?}",
        r2.diagnostics
    );
}

#[test]
fn string_spread_into_non_byte_slice_rejected_with_e1002() {
    let src = "\
fn main() {
    xs := []float64{1.0}
    zs := append(xs, \"hello\"...)
    _ = zs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for string spread into []float64, got {:?}",
        r.diagnostics
    );
}

#[test]
fn non_slice_non_string_spread_rejected_with_e1002() {
    let src = "\
fn main() {
    xs := []int{1}
    zs := append(xs, 42...)
    _ = zs
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for non-slice spread, got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_const_and_matching_spread_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20
    xs := []float64{}
    _ = append(xs, 1)
    _ = append(xs, local_k)
    _ = append(xs, file_k)
    _ = append(xs, 2 + 3)

    ints := []int{1}
    more_ints := []int{2, 3}
    _ = append(ints, more_ints...)
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for valid untyped constants and matching spread, got diags: {:?}",
        r.diagnostics
    );
}
