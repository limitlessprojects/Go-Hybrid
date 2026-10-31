//! composite_literals_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables in composite literals (struct, slice, array, map)
//! for float targets, while preserving untyped integer constants acceptance.

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
fn struct_literal_float_field_with_typed_int_is_rejected() {
    let src = r#"
struct point {
    x float64
}

fn test_struct_lit() {
    var i int = 1
    p := point{x: i}
    _ = p
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
        res.diagnostics.iter().any(|d| d.message.contains("struct literal")),
        "diagnostic should mention struct literal: {:?}",
        res.diagnostics
    );
}

#[test]
fn slice_literal_float_with_typed_int_is_rejected() {
    let src = r#"
fn test_slice_lit() {
    var i int = 1
    s := []float64{i}
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
        res.diagnostics.iter().any(|d| d.message.contains("slice literal")),
        "diagnostic should mention slice literal: {:?}",
        res.diagnostics
    );
}

#[test]
fn array_literal_float_with_typed_int_is_rejected() {
    let src = r#"
fn test_array_lit() {
    var i int = 1
    a := [1]float64{i}
    _ = a
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
        res.diagnostics.iter().any(|d| d.message.contains("array literal")),
        "diagnostic should mention array literal: {:?}",
        res.diagnostics
    );
}

#[test]
fn map_literal_float_key_and_value_with_typed_int_is_rejected() {
    let src = r#"
fn test_map_key_lit() {
    var i int = 1
    m1 := map[float64]string{i: "hello"}
    _ = m1
}

fn test_map_val_lit() {
    var i int = 1
    m2 := map[string]float64{"hello": i}
    _ = m2
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    let diags = error_codes(&res);
    assert!(
        diags.iter().filter(|&c| c == "HYB-E1002").count() >= 2,
        "expected at least 2 HYB-E1002 errors, got {:?}",
        res.diagnostics
    );
}

#[test]
fn composite_literals_with_untyped_int_constants_are_accepted() {
    let src = r#"
const k = 2

struct point {
    x float64
}

fn test_constants() {
    p1 := point{x: 1}
    p2 := point{x: k}
    p3 := point{x: k + 1}
    _ = p1
    _ = p2
    _ = p3

    s1 := []float64{1, 2}
    s2 := []float64{k, k + 1}
    _ = s1
    _ = s2

    a1 := [2]float64{1, 2}
    a2 := [2]float64{k, k + 1}
    _ = a1
    _ = a2

    m1 := map[float64]float64{1: 2, k: k + 1}
    _ = m1
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
