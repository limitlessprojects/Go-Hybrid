//! assign_and_map_index_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables/constants assigned to float64 targets and
//! indexing map[float64], while preserving untyped integer constants.
//!
//! Regression test for Issue #2567.

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
fn single_assignment_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_single_assign() {
    var f float64
    var i int = 1
    f = i
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `int` to `f` (type `float64`)")),
        "diagnostic should mention assigning int to float: {:?}",
        res.diagnostics
    );
}

#[test]
fn parallel_assignment_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_parallel_assign() {
    var f float64
    var s string
    var i int = 1
    f, s = i, "hello"
    _ = f
    _ = s
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
        res.diagnostics.iter().any(|d| d.message.contains("parallel assignment")),
        "diagnostic should mention parallel assignment: {:?}",
        res.diagnostics
    );
}

#[test]
fn map_float_index_with_typed_int_is_rejected() {
    let src = r#"
fn test_map_index() {
    var m map[float64]string
    var i int = 1
    _ = m[i]
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map index")),
        "diagnostic should mention map index mismatch: {:?}",
        res.diagnostics
    );
}

#[test]
fn typed_int_const_in_assign_and_map_index_is_rejected() {
    let src = r#"
const k int = 1

fn test_typed_const() {
    var f float64
    f = k
    _ = f

    var m map[float64]string
    _ = m[k]
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "typed int const should be rejected in assignment and map index: {:?}",
        res.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_assign_and_map_index_are_accepted() {
    let src = r#"
const k = 3

fn test_constants_allowed() {
    var f float64
    f = 1
    f = k
    f = k + 2
    _ = f

    var m map[float64]string
    _ = m[1]
    _ = m[k]
    _ = m[k + 1]
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal for float targets: {:?}",
        res.diagnostics
    );
}
