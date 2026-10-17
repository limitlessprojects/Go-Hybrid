//! addr_of_loan.rs — regression tests for T24 (HYB-E3104): no address-of
//! on a loan. SPEC-LANGUAGE.md §2.9 pins `p := &n` (with `n := &g.nodes[from]`)
//! as a compile-fail claim; before this check it was accepted with
//! 0 diagnostics and emitted a general Go pointer to a copy of the slot.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn main_with(body: &str) -> String {
    format!("fn main() {{\n{body}}}\n")
}

#[test]
fn spec_t24_addr_of_map_slot_loan_is_rejected_e3104() {
    let r = check(&main_with(
        "    g := map[string]int{\"a\": 1}\n    n := &g[\"a\"]\n    p := &n\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E3104".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn spec_t24_struct_field_map_shape_is_rejected_e3104() {
    // The exact SPEC shape: a loan of a map held in a struct field.
    let src = "struct graph { nodes map[int]int }\n\nfn main() {\n    g := graph{nodes: map[int]int{1: 2}}\n    from := 1\n    n := &g.nodes[from]\n    p := &n\n    fmt.println(p)\n}\n";
    let r = check(src);
    assert_eq!(codes(&r), vec!["HYB-E3104".to_string()]);
}

#[test]
fn addr_of_loan_as_call_argument_is_rejected_e3104() {
    let r = check(&main_with(
        "    g := map[string]int{\"a\": 1}\n    n := &g[\"a\"]\n    fmt.sscan(\"7\", &n)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E3104".to_string()]);
}

#[test]
fn addr_of_parenthesized_loan_is_rejected_e3104() {
    let r = check(&main_with(
        "    g := map[string]int{\"a\": 1}\n    n := &g[\"a\"]\n    p := &(n)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E3104".to_string()]);
}

#[test]
fn using_the_loan_itself_is_accepted() {
    // The T24 "ok" line: the slot loan is the legal `&`.
    let r = check(&main_with(
        "    g := map[string]int{\"a\": 1}\n    n := &g[\"a\"]\n    fmt.println(n)\n",
    ));
    assert!(
        codes(&r).is_empty(),
        "unexpected errors: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn shadowing_non_loan_binding_is_not_a_loan() {
    // An inner `n := 5` is a fresh binding, not the outer slot loan:
    // whatever `&n` means for a plain local, it is not T24's E3104.
    let r = check(&main_with(
        "    g := map[string]int{\"a\": 1}\n    n := &g[\"a\"]\n    fmt.println(n)\n    if true {\n        n := 5\n        p := &n\n        fmt.println(p)\n    }\n",
    ));
    assert!(
        !codes(&r).iter().any(|c| c == "HYB-E3104"),
        "a shadowing non-loan binding must not raise E3104: {:?}",
        r.diagnostics
    );
}

#[test]
fn addr_of_plain_local_is_not_affected() {
    let r = check(&main_with("    x := 5\n    p := &x\n    fmt.println(p)\n"));
    assert!(
        !codes(&r).iter().any(|c| c == "HYB-E3104"),
        "E3104 is for loans only: {:?}",
        r.diagnostics
    );
}
