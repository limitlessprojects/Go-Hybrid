//! type_elided_struct_literals.rs — tests for Issue #2098:
//! [Soundness] hy-check: type-elided struct literals {...} bypass check_struct_literal
//! (U14/U15/field-type checks) and scalar composite checks (HYB-E1002 / Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got errors: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn slice_elided_struct_unknown_field_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    s := []item{{unknown_field: 1}}\n    _ = s\n}\n";
    assert_rejects_e1002(&check(src), "[]item{{unknown_field: 1}}");
}

#[test]
fn slice_elided_struct_duplicate_field_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    s := []item{{count: 1, count: 2}}\n    _ = s\n}\n";
    assert_rejects_e1002(&check(src), "[]item{{count: 1, count: 2}}");
}

#[test]
fn slice_elided_struct_field_type_mismatch_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    s := []item{{count: \"wrong_type\"}}\n    _ = s\n}\n";
    assert_rejects_e1002(&check(src), "[]item{{count: \"wrong_type\"}}");
}

#[test]
fn array_elided_struct_unknown_field_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    a := [1]item{{unknown_field: 1}}\n    _ = a\n}\n";
    assert_rejects_e1002(&check(src), "[1]item{{unknown_field: 1}}");
}

#[test]
fn array_elided_struct_field_type_mismatch_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    a := [1]item{{count: \"wrong\"}}\n    _ = a\n}\n";
    assert_rejects_e1002(&check(src), "[1]item{{count: \"wrong\"}}");
}

#[test]
fn map_value_elided_struct_unknown_field_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    m := map[string]item{\"k\": {unknown_field: 1}}\n    _ = m\n}\n";
    assert_rejects_e1002(&check(src), "map[string]item{\"k\": {unknown_field: 1}}");
}

#[test]
fn map_value_elided_struct_field_type_mismatch_rejected() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    m := map[string]item{\"k\": {count: \"wrong\"}}\n    _ = m\n}\n";
    assert_rejects_e1002(&check(src), "map[string]item{\"k\": {count: \"wrong\"}}");
}

#[test]
fn map_key_elided_struct_unknown_field_rejected() {
    let src = "struct item {\n    id int\n}\nfn main() {\n    m := map[item]string{{bad: 1}: \"v\"}\n    _ = m\n}\n";
    assert_rejects_e1002(&check(src), "map[item]string{{bad: 1}: \"v\"}");
}

#[test]
fn slice_scalar_elided_struct_rejected() {
    let src = "fn main() {\n    s := []int{{x: 1}}\n    _ = s\n}\n";
    assert_rejects_e1002(&check(src), "[]int{{x: 1}}");
}

#[test]
fn array_scalar_elided_struct_rejected() {
    let src = "fn main() {\n    a := [1]int{{x: 1}}\n    _ = a\n}\n";
    assert_rejects_e1002(&check(src), "[1]int{{x: 1}}");
}

#[test]
fn map_value_scalar_elided_struct_rejected() {
    let src = "fn main() {\n    m := map[string]int{\"k\": {x: 1}}\n    _ = m\n}\n";
    assert_rejects_e1002(&check(src), "map[string]int{\"k\": {x: 1}}");
}

#[test]
fn map_key_scalar_elided_struct_rejected() {
    let src = "fn main() {\n    m := map[int]string{{x: 1}: \"v\"}\n    _ = m\n}\n";
    assert_rejects_e1002(&check(src), "map[int]string{{x: 1}: \"v\"}");
}

#[test]
fn nested_struct_elided_field_checks() {
    let src_unknown = "struct inner {\n    val int\n}\nstruct outer {\n    child inner\n}\nfn main() {\n    o := outer{child: {unknown: 1}}\n    _ = o\n}\n";
    assert_rejects_e1002(&check(src_unknown), "outer{child: {unknown: 1}}");

    let src_type = "struct inner {\n    val int\n}\nstruct outer {\n    child inner\n}\nfn main() {\n    o := outer{child: {val: \"str\"}}\n    _ = o\n}\n";
    assert_rejects_e1002(&check(src_type), "outer{child: {val: \"str\"}}");

    let src_scalar = "struct outer {\n    score int\n}\nfn main() {\n    o := outer{score: {x: 1}}\n    _ = o\n}\n";
    assert_rejects_e1002(&check(src_scalar), "outer{score: {x: 1}}");
}

#[test]
fn pointer_to_struct_elided_literals() {
    let src_ok = "struct item {\n    count int\n}\nfn main() {\n    s := []*item{{count: 1}}\n    _ = s\n}\n";
    assert_ok(src_ok, "[]*item{{count: 1}} accepted");

    let src_bad = "struct item {\n    count int\n}\nfn main() {\n    s := []*item{{bad_field: 1}}\n    _ = s\n}\n";
    assert_rejects_e1002(&check(src_bad), "[]*item{{bad_field: 1}}");
}

#[test]
fn valid_elided_struct_literals_accepted() {
    let src = "struct item {\n    count int\n    label string\n}\nfn main() {\n    s := []item{\n        {count: 1, label: \"ok\"},\n        {},\n    }\n    a := [2]item{\n        {count: 2},\n        {},\n    }\n    m := map[string]item{\n        \"k1\": {count: 3},\n        \"k2\": {},\n    }\n    _ = s\n    _ = a\n    _ = m\n}\n";
    assert_ok(src, "valid elided struct literals");
}
