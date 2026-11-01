//! issue_2945_single_var_map_range_key_binding.rs
//! Regression test for issue #2945:
//! Single-variable range loop `for k in m` over a map must bind the map's KEY,
//! as mandated by SPEC-LANGUAGE §1.3 (rule LX13).
//! Previously, hy-check and hy-emit bound the map VALUE for single-variable loops.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn single_var_map_range_binds_key_type_accepts() {
    let src = r#"
fn main() {
    m := map[string]int{"hello": 1, "world": 2}
    for k in m {
        var s string = k
        _ = s
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for valid key binding");
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", error_codes(&r));
}

#[test]
fn single_var_map_range_rejects_value_type_assignment() {
    let src = r#"
fn main() {
    m := map[string]int{"hello": 1, "world": 2}
    for k in m {
        var n int = k
        _ = n
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "must reject assigning key (string) to int");
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 type mismatch, got {:?}",
        codes
    );
}

#[test]
fn two_var_map_range_binds_key_and_value_accepts() {
    let src = r#"
fn main() {
    m := map[string]int{"hello": 1, "world": 2}
    for k, v in m {
        var s string = k
        var n int = v
        _ = s
        _ = n
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for two-var key and value binding");
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", error_codes(&r));
}

#[test]
fn single_var_literal_map_range_binds_key() {
    let src = r#"
fn main() {
    for k in map[int]string{1: "a", 2: "b"} {
        var n int = k
        _ = n
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "literal map single-var binds key int");
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", error_codes(&r));
}

#[test]
fn single_var_empty_map_range_binds_key() {
    let src = r#"
fn main() {
    m := map[int]bool{}
    for k in m {
        var n int = k
        _ = n
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "empty map single-var binds key int");
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", error_codes(&r));
}
