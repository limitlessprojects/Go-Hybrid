//! match_duplicate_case_soundness.rs — tests for HYB-E1002 rejection of
//! duplicate case patterns in match statements (Row 13 BUILDABLE).

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
fn match_duplicate_int_literal_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    match x {\n        1 => { _ = 1 }\n        1 => { _ = 2 }\n        _ => { _ = 3 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_duplicate_int_hex_literal_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    match x {\n        10 => { _ = 1 }\n        0xa => { _ = 2 }\n        _ => { _ = 3 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_duplicate_str_literal_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    match s {\n        \"hello\" => { _ = 1 }\n        \"hello\" => { _ = 2 }\n        _ => { _ = 3 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_duplicate_bool_literal_rejects_e1002() {
    let src = "fn main() {\n    b := true\n    match b {\n        true => { _ = 1 }\n        true => { _ = 2 }\n        _ => { _ = 3 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_duplicate_enum_arm_rejects_e1002() {
    let src = "enum color { red, green, blue }\nfn main() {\n    c := color.red\n    match c {\n        color.red => { _ = 1 }\n        color.red => { _ = 2 }\n        color.green => { _ = 3 }\n        color.blue => { _ = 4 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_duplicate_option_some_arm_rejects_e1002() {
    let src = "fn main() {\n    x := some(1)\n    match x {\n        some(v) => { _ = v }\n        some(w) => { _ = w }\n        none => { _ = 0 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn match_guarded_duplicate_literal_accepted() {
    let src = "fn main() {\n    x := 1\n    match x {\n        1 if x > 0 => { _ = 1 }\n        1 if x <= 0 => { _ = 2 }\n        _ => { _ = 3 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn match_distinct_arms_accepted() {
    let src = "fn main() {\n    x := 1\n    match x {\n        1 => { _ = 1 }\n        2 => { _ = 2 }\n        3 => { _ = 3 }\n        _ => { _ = 4 }\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
