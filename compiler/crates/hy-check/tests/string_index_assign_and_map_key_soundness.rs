//! string_index_assign_and_map_key_soundness.rs — tests for HYB-E1002 rejection of
//! string index assignments (mutation of immutable strings) and map index key type mismatches.

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
fn string_index_assign_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    s[0] = 65\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_index_inc_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    s[0]++\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_index_dec_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    s[0]--\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_index_compound_assign_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    s[0] += 1\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_index_parallel_assign_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    s[0], s[1] = 65, 66\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn struct_string_field_index_assign_rejects_e1002() {
    let src = "struct holder {\n    s string\n}\nfn main() {\n    obj := holder{s: \"hello\"}\n    obj.s[0] = 65\n    _ = obj\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_int_key_with_string_index_rejects_e1002() {
    let src = "fn main() {\n    m := map[int]string{1: \"a\"}\n    _ = m[\"bad\"]\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_string_key_with_int_index_rejects_e1002() {
    let src = "fn main() {\n    m := map[string]int{\"a\": 1}\n    _ = m[123]\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_string_key_with_int_assign_rejects_e1002() {
    let src = "fn main() {\n    m := map[string]int{\"a\": 1}\n    m[123] = 2\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_bool_key_with_int_index_rejects_e1002() {
    let src = "fn main() {\n    m := map[bool]int{true: 1}\n    _ = m[42]\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_void_call_index_rejects_e1002() {
    let src = "fn noop() {}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    _ = m[noop()]\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn slice_void_bound_rejects_e1002() {
    let src = "fn noop() {}\nfn main() {\n    s := \"hello\"\n    _ = s[noop():]\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_string_read_and_slice_accepts() {
    let src = "fn main() {\n    s := \"hello\"\n    b := s[0]\n    sub := s[1:3]\n    _ = b\n    _ = sub\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn valid_map_read_and_write_accepts() {
    let src = "fn main() {\n    m := map[string]int{\"a\": 1}\n    m[\"b\"] = 2\n    v := m[\"a\"]\n    _ = v\n    _ = m\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
