//! composite_literals_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables/constants in composite literals (struct, slice,
//! array, map) for float64 targets, while preserving untyped integer constants.
//!
//! Regression test for Issue #2573.

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
fn struct_literal_typed_int_variable_is_rejected() {
    let src = r#"
struct point {
    x float64
}

fn test_struct() {
    var i int = 1
    _ = point{x: i}
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use a value of type `int` for field `x`")),
        "diagnostic should mention int value for float field: {:?}",
        res.diagnostics
    );
}

#[test]
fn slice_literal_typed_int_variable_is_rejected() {
    let src = r#"
fn test_slice() {
    var i int = 1
    _ = []float64{i}
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in slice literal")),
        "diagnostic should mention slice literal type mismatch: {:?}",
        res.diagnostics
    );
}

#[test]
fn array_literal_typed_int_variable_is_rejected() {
    let src = r#"
fn test_array() {
    var i int = 1
    _ = [1]float64{i}
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in array literal")),
        "diagnostic should mention array literal type mismatch: {:?}",
        res.diagnostics
    );
}

#[test]
fn map_literal_typed_int_key_and_value_are_rejected() {
    let src_key = r#"
fn test_map_key() {
    var i int = 1
    _ = map[float64]string{i: "val"}
}
"#;
    let res_key = check(src_key);
    assert_eq!(res_key.verdict, Verdict::Reject);
    assert!(
        error_codes(&res_key).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for map key: {:?}",
        res_key.diagnostics
    );
    assert!(
        res_key.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map key")),
        "diagnostic should mention map key type mismatch: {:?}",
        res_key.diagnostics
    );

    let src_val = r#"
fn test_map_val() {
    var i int = 1
    _ = map[string]float64{"k": i}
}
"#;
    let res_val = check(src_val);
    assert_eq!(res_val.verdict, Verdict::Reject);
    assert!(
        error_codes(&res_val).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for map val: {:?}",
        res_val.diagnostics
    );
    assert!(
        res_val.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map value")),
        "diagnostic should mention map val type mismatch: {:?}",
        res_val.diagnostics
    );
}

#[test]
fn composite_literals_typed_int_const_is_rejected() {
    let src = r#"
const k int = 1

struct point {
    x float64
}

fn test_typed_const() {
    _ = point{x: k}
    _ = []float64{k}
    _ = [1]float64{k}
    _ = map[float64]string{k: "val"}
    _ = map[string]float64{"k": k}
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "typed int const should be rejected for float targets in composite literals: {:?}",
        res.diagnostics
    );
}

#[test]
fn composite_literals_untyped_constants_are_accepted() {
    let src = r#"
const k = 1

struct point {
    x float64
}

fn test_untyped_constants() {
    _ = point{x: 1}
    _ = point{x: k}
    _ = point{x: k + 2}
    _ = point{x: -1}

    _ = []float64{1, k, k + 2, -1, k * 3}
    _ = [5]float64{1, k, k + 2, -1, k * 3}

    _ = map[float64]string{10: "a", k + 1: "b", k + 3: "c"}
    _ = map[string]float64{"a": 1, "b": k, "c": k + 2}
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal in composite literals: {:?}",
        res.diagnostics
    );
}
