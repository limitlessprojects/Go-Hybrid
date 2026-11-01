//! issue_3090_match_pattern_soundness.rs — tests for HYB-E1002 rejection of
//! `nil` match arm patterns on non-nilable types and out-of-range integer patterns.

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
fn match_int_with_nil_pattern_rejects_e1002() {
    let src = "fn main() {\n    x := 5\n    match x {\n        nil => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_string_with_nil_pattern_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    match s {\n        nil => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_bool_with_nil_pattern_rejects_e1002() {
    let src = "fn main() {\n    b := true\n    match b {\n        nil => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_struct_with_nil_pattern_rejects_e1002() {
    let src = "struct item {\n    val int\n}\nfn main() {\n    it := item{val: 1}\n    match it {\n        nil => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_uint8_with_overflowing_integer_rejects_e1002() {
    let src = "fn main() {\n    var u uint8 = 10\n    match u {\n        300 => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_valid_pointer_nil_and_integers_accepts() {
    let src = "fn main() {\n    var p *int = nil\n    match p {\n        nil => {}\n        _ => {}\n    }\n    var u uint8 = 10\n    match u {\n        20 => {}\n        _ => {}\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
