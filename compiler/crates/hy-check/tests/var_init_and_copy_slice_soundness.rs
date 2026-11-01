//! var_init_and_copy_slice_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables initializing float64 variable declarations,
//! and mismatched element types in built-in `copy(dst, src)`.

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
fn single_var_init_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_var_init() {
    var i int = 1
    var f float64 = i
    _ = f
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
        res.diagnostics.iter().any(|d| d.message.contains("variable declaration")),
        "diagnostic should mention variable declaration: {:?}",
        res.diagnostics
    );
}

#[test]
fn let_init_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_let_init() {
    var i int = 1
    let f float64 = i
    _ = f
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
        res.diagnostics.iter().any(|d| d.message.contains("variable declaration")),
        "diagnostic should mention variable declaration: {:?}",
        res.diagnostics
    );
}

#[test]
fn copy_slice_mismatched_float_and_int_is_rejected() {
    let src1 = r#"
fn test_copy_float_int() {
    var dst []float64
    var src []int
    copy(dst, src)
}
"#;
    let res1 = check(src1);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(
        error_codes(&res1).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res1.diagnostics
    );
    assert!(
        res1.diagnostics.iter().any(|d| d.message.contains("different element types")),
        "diagnostic should mention different element types: {:?}",
        res1.diagnostics
    );

    let src2 = r#"
fn test_copy_int_float() {
    var dst []int
    var src []float64
    copy(dst, src)
}
"#;
    let res2 = check(src2);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(
        error_codes(&res2).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res2.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_float_var_init_accepted() {
    let src = r#"
const k = 2

fn test_valid_inits() {
    var f1 float64 = 1
    var f2 float64 = k
    var f3 float64 = k + 1
    const local_c = 3
    var f4 float64 = local_c
    var dst []float64
    var src []float64
    copy(dst, src)
    _ = f1
    _ = f2
    _ = f3
    _ = f4
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal in float var init: {:?}",
        res.diagnostics
    );
}

#[test]
fn typed_int_constants_in_float_var_init_rejected() {
    let src_file_const = r#"
const k int = 1

fn test_file_typed_const() {
    var f float64 = k
    _ = f
}
"#;
    let res1 = check(src_file_const);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(
        error_codes(&res1).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for file-level typed const init, got {:?}",
        res1.diagnostics
    );

    let src_local_const = r#"
fn test_local_typed_const() {
    const k int = 1
    var f float64 = k
    _ = f
}
"#;
    let res2 = check(src_local_const);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(
        error_codes(&res2).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for local typed const init, got {:?}",
        res2.diagnostics
    );
}
