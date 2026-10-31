//! addr_of_rvalue_projections.rs — regression tests verifying that composite
//! literals are only valid as the direct (possibly parenthesized) operand of
//! `&` and are NOT addressable as the base of field or array-index projections.

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
fn rejects_addr_of_struct_literal_field_in_expr_and_binding() {
    let src = "\
struct box {
    v int
}

fn take(p *int) {}

fn main() {
    take(&(box{v: 1}.v))
    p := &(box{v: 1}.v)
    _ = p
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert_eq!(
        codes.iter().filter(|c| c.as_str() == "HYB-E1002").count(),
        2,
        "expected two HYB-E1002 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn rejects_addr_of_array_literal_element_and_nested_projection() {
    let src = "\
struct outer {
    arr [2]int
}

fn main() {
    p1 := &([2]int{1, 2}[0])
    p2 := &(outer{arr: [2]int{1, 2}}.arr[0])
    _ = p1
    _ = p2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert_eq!(
        codes.iter().filter(|c| c.as_str() == "HYB-E1002").count(),
        2,
        "expected two HYB-E1002 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn accepts_direct_addr_of_composite_literal_and_slice_literal_index() {
    let src = "\
struct box {
    v int
}

fn main() {
    p1 := &box{v: 1}
    p2 := &(box{v: 2})
    p3 := &[2]int{1, 2}
    p4 := &([]int{1, 2}[0])
    _ = p1
    _ = p2
    _ = p3
    _ = p4
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "unexpected diagnostics: {:?}",
        r.diagnostics
    );
}
