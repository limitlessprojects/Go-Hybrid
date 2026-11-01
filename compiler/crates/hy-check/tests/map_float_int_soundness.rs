//! map_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables used as float64 keys or values in maps
//! (index expressions, composite literal keys, composite literal values),
//! while preserving untyped integer constants adaptation.

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
fn map_index_typed_int_on_float_key_is_rejected() {
    let src = r#"
fn test_map_index() {
    var m map[float64]string
    var i int = 1
    _ = m[i]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for m[i], got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map index on `map[float64]string`")));
}

#[test]
fn map_lit_key_typed_int_on_float_key_is_rejected() {
    let src = r#"
fn test_map_lit_key() {
    var i int = 1
    var m map[float64]string = map[float64]string{ i: "a" }
    _ = m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for map literal key, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map key")));
}

#[test]
fn map_lit_val_typed_int_on_float_val_is_rejected() {
    let src = r#"
fn test_map_lit_val() {
    var i int = 1
    var m map[string]float64 = map[string]float64{ "a": i }
    _ = m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for map literal value, got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in map value")));
}

#[test]
fn map_untyped_int_constants_stay_accepted() {
    let src = r#"
const key_one = 10
const val_two = 20

fn test_constants() {
    var m1 map[float64]string
    _ = m1[1]
    _ = m1[key_one]

    var m2 map[float64]string = map[float64]string{ 1: "a", key_one: "b" }
    var m3 map[string]float64 = map[string]float64{ "a": 1, "b": val_two }
    _ = m2
    _ = m3
}
"#;
    let r = check(src);
    let errs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == hy_diag::Severity::Error).collect();
    assert!(
        errs.is_empty(),
        "untyped int constants in maps must remain accepted, got: {:?}",
        errs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
