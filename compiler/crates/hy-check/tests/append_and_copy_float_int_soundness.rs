//! append_and_copy_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables passed to append([]float64, ...) and copy([]float64, []int),
//! while preserving untyped integer constants acceptance.

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn append_typed_int_to_float_slice_is_rejected() {
    let src = r#"
fn test_append() {
    var xs []float64
    var i int = 1
    xs = append(xs, i)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("argument to append")),
        "diagnostic should mention argument to append: {:?}",
        res.diagnostics
    );
}

#[test]
fn copy_float_and_int_slices_is_rejected() {
    let src = r#"
fn test_copy() {
    var dst []float64
    var src []int
    copy(dst, src)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("arguments to `copy` have different element types")),
        "diagnostic should mention copy element types: {:?}",
        res.diagnostics
    );
}

#[test]
fn append_untyped_int_constants_to_float_slice_is_accepted() {
    let src = r#"
const k = 2

fn test_append_const() {
    var xs []float64
    xs = append(xs, 1)
    xs = append(xs, k)
    xs = append(xs, k + 1)
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal in append to float slice: {:?}",
        res.diagnostics
    );
}
