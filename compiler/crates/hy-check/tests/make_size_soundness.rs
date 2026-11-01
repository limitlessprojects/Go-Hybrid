//! make_size_soundness.rs — regression tests for Issue #2889.
//!
//! Built-in `make` requires size arguments (length and capacity) to be integer types
//! or untyped integer constants. Variable expressions of non-integer types (e.g. float64,
//! string, bool) must be rejected with HYB-E2005.

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
fn make_slice_len_float_var_rejected() {
    let src = r#"
fn test_make() {
    var f float64 = 1.5
    s := make([]int32, f)
    _ = s
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for float length variable in make, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_slice_cap_float_var_rejected() {
    let src = r#"
fn test_make() {
    var c float64 = 5.5
    s := make([]int32, 10, c)
    _ = s
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for float capacity variable in make, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_slice_string_var_rejected() {
    let src = r#"
fn test_make() {
    var s string = "10"
    xs := make([]int32, s)
    _ = xs
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for string length variable in make, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_slice_bool_var_rejected() {
    let src = r#"
fn test_make() {
    var b bool = true
    xs := make([]int32, b)
    _ = xs
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for bool length variable in make, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_chan_cap_float_var_rejected() {
    let src = r#"
fn test_make() {
    var f float64 = 2.0
    ch := make(chan int32, f)
    _ = ch
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for float capacity variable in make chan, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_chan_cap_string_var_rejected() {
    let src = r#"
fn test_make() {
    var s string = "4"
    ch := make(chan int32, s)
    _ = ch
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for string capacity variable in make chan, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_map_size_float_var_rejected() {
    let src = r#"
type cache_map = map[string]int32

fn test_make() {
    var f float64 = 4.0
    m := make(cache_map, f)
    _ = m
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for float size variable in make map, got {:?}",
        res.diagnostics
    );
}

#[test]
fn make_integer_variables_accepted() {
    let src = r#"
type cache_map = map[string]int32

fn test_make() {
    var n int = 10
    var m int64 = 20
    s := make([]int32, n, m)
    _ = s

    var k int32 = 5
    ch := make(chan int32, k)
    _ = ch

    var cap_val int = 8
    mp := make(cache_map, cap_val)
    _ = mp
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected zero diagnostics, got {:?}", res.diagnostics);
}
