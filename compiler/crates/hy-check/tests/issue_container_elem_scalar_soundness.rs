//! issue_container_elem_scalar_soundness.rs — Row 13 BUILDABLE soundness.
//!
//! Regression tests for HYB-E1002: container element scalar width mismatches
//! ([]int64 → []int32, map[K]int64 → map[K]int32) that `kinds_conflict`
//! cannot see because `OpKind::Slice(Int) == Slice(Int)`.

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
fn call_arg_int64_slice_to_int32_param_rejected() {
    let r = check(
        r#"fn get_int64s() []int64 {
    var res []int64
    return res
}
fn takes_int32_slice(x []int32) {}
fn main() {
    takes_int32_slice(get_int64s())
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn call_arg_int32_slice_to_int64_param_rejected() {
    let r = check(
        r#"fn get_int32s() []int32 {
    var res []int32
    return res
}
fn takes_int64_slice(x []int64) {}
fn main() {
    takes_int64_slice(get_int32s())
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn var_decl_int64_slice_assigned_to_int32_rejected() {
    let r = check(
        r#"fn get_int64s() []int64 {
    var res []int64
    return res
}
fn main() {
    var s []int32 = get_int64s()
    _ = s
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn return_int64_slice_from_int32_result_rejected() {
    let r = check(
        r#"fn get_int64s() []int64 {
    var res []int64
    return res
}
fn returns_int32_slice() []int32 {
    return get_int64s()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn struct_literal_int64_slice_to_int32_field_rejected() {
    let r = check(
        r#"fn get_int64s() []int64 {
    var res []int64
    return res
}
struct config {
    nums []int32
}
fn main() {
    c := config{ nums: get_int64s() }
    _ = c
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn call_arg_same_element_type_accepted() {
    let r = check(
        r#"fn get_int32s() []int32 {
    var res []int32
    return res
}
fn takes_int32_slice(x []int32) {}
fn main() {
    takes_int32_slice(get_int32s())
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected accept: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected no diagnostics: {:?}", r.diagnostics);
}
