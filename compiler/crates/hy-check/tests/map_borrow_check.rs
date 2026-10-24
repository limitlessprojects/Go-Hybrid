//! map_borrow_check.rs — tests for Go-Hybrid #1046 (projected map-element borrows).

use hy_check::check_file;
use hy_syntax::parse_file;

fn rejects_with_e3104(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e3104 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3104" && d.severity == hy_diag::Severity::Error);
    assert!(
        has_e3104,
        "expected HYB-E3104 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

#[test]
fn projected_map_element_field_borrow_issue_1046() {
    rejects_with_e3104(
        "struct acct { bal int }\n\
         fn touch(p *int) { _ = p }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             touch(&m[\"a\"].bal)\n\
         }\n"
    );
}

#[test]
fn projected_map_element_array_index_borrow_issue_1046() {
    rejects_with_e3104(
        "struct vec { arr [2]int }\n\
         fn touch(p *int) { _ = p }\n\
         fn main() {\n\
             m := map[string]vec{\"a\": vec{arr: [2]int{1, 2}}}\n\
             touch(&m[\"a\"].arr[0])\n\
         }\n"
    );
}
