//! issue_2858_typed_int_index_field.rs — typed integers read through an
//! index, map lookup, struct field or string index keep their declared width
//! for the #2712 HYB-E1002 check (fixes #2858).

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

fn assert_rejects(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}\n{:?}", src, r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "{:?}", r.diagnostics);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "expected no errors for:\n{}\n{:?}", src, r.diagnostics);
}

#[test]
fn slice_element_width_mismatch_rejects_e1002() {
    assert_rejects("fn main() {\n    s := []i64{1}\n    var a int = s[0]\n    _ = a\n}\n");
}

#[test]
fn map_value_width_mismatch_rejects_e1002() {
    assert_rejects("fn main() {\n    m := map[string]i64{\"a\": 1}\n    var b int = m[\"a\"]\n    _ = b\n}\n");
}

#[test]
fn struct_field_width_mismatch_rejects_e1002() {
    assert_rejects("struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var c int = p.v\n    _ = c\n}\n");
}

#[test]
fn define_from_index_carries_width_rejects_e1002() {
    assert_rejects("fn main() {\n    s := []i64{1}\n    x := s[0]\n    var d int = x\n    _ = d\n}\n");
}

#[test]
fn string_index_is_byte_not_rune_rejects_e1002() {
    assert_rejects("fn main() {\n    str := \"abc\"\n    var r rune = str[0]\n    _ = r\n}\n");
}

#[test]
fn typed_param_slice_and_pointer_field_reject_e1002() {
    assert_rejects("fn f(s []i32) int {\n    return s[0]\n}\nfn main() {\n    _ = f([]i32{1})\n}\n");
    assert_rejects("struct pt {\n    v u8\n}\nfn g(p *pt) {\n    var n int = p.v\n    _ = n\n}\nfn main() {\n    g(&pt{v: 1})\n}\n");
}

#[test]
fn matching_widths_and_aliases_accept() {
    assert_accepts("fn main() {\n    s := []i64{1}\n    var a i64 = s[0]\n    _ = a\n}\n");
    assert_accepts("fn main() {\n    m := map[string]int{\"a\": 1}\n    var b int = m[\"a\"]\n    _ = b\n}\n");
    assert_accepts("struct pt {\n    v i64\n}\nfn main() {\n    p := pt{v: 1}\n    var c i64 = p.v\n    x := p.v + 1\n    var d i64 = x\n    _ = c\n    _ = d\n}\n");
    assert_accepts("fn main() {\n    str := \"abc\"\n    var b byte = str[0]\n    var u u8 = str[1]\n    _ = b\n    _ = u\n}\n");
}

#[test]
fn inner_rebinding_does_not_inherit_outer_type_text() {
    assert_accepts(
        "fn mk() []int {\n    return []int{1}\n}\nfn main() {\n    s := []i64{1}\n    _ = s\n    if true {\n        s := mk()\n        var a int = s[0]\n        _ = a\n    }\n}\n",
    );
}
