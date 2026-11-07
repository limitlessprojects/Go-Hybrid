//! issue_3218_fn_lit_map_lit_soundness.rs — regression tests for #3218:
//! check_fn_lit_call and map literal entry checks must enforce scalar widths,
//! fixed-size array lengths, and multi-value call argument types with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_fn_lit_scalar_mismatch_int64_to_int32_rejects() {
    let src = r#"
fn test() {
    var v int64 = 1
    (fn(a int32) {})(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int64 to fn(int32)");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_fn_lit_scalar_mismatch_float64_to_float32_rejects() {
    let src = r#"
fn test() {
    var v float64 = 1.0
    (fn(a float32) {})(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for float64 to fn(float32)");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_fn_lit_int_to_float_typed_var_rejects() {
    let src = r#"
fn test() {
    var v int = 1
    (fn(a float64) {})(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for int var to fn(float64)");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_fn_lit_array_len_mismatch_rejects() {
    let src = r#"
fn test() {
    var a [3]int
    (fn(x [2]int) {})(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int to fn([2]int)");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_fn_lit_multi_value_call_mismatch_rejects() {
    let src = r#"
fn pair() (int, string) {
    return 1, "a"
}

fn test() {
    (fn(a int, b int) {})(pair())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for multi-value call type mismatch to fn lit");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_map_lit_key_array_len_mismatch_rejects() {
    let src = r#"
fn test() {
    _ = map[[2]int]string{[3]int{1, 2, 3}: "ok"}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int key in map[[2]int]string");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_map_lit_val_array_len_mismatch_rejects() {
    let src = r#"
fn test() {
    _ = map[string][2]int{"k": [3]int{1, 2, 3}}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int value in map[string][2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_map_lit_val_array_var_len_mismatch_rejects() {
    let src = r#"
fn test() {
    var a [3]int
    _ = map[string][2]int{"k": a}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for [3]int variable in map[string][2]int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_fn_lit_matching_args_accepts() {
    let src = r#"
fn test() {
    var v int32 = 1
    (fn(a int32) {})(v)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching int32, got {:?}", r.diagnostics);
}

#[test]
fn test_fn_lit_untyped_int_const_accepts() {
    let src = r#"
fn test() {
    (fn(a int32) {})(10)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for untyped int const, got {:?}", r.diagnostics);
}

#[test]
fn test_map_lit_matching_array_accepts() {
    let src = r#"
fn test() {
    _ = map[string][2]int{"k": [2]int{1, 2}}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching [2]int, got {:?}", r.diagnostics);
}
