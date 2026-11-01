//! multi_value_index_slice_base.rs - regression tests for a multi-value call
//! used as the BASE of an index or slice expression (GUARANTEES.md Row 13,
//! BUILDABLE).
//!
//! `check_index_kind` and `check_slice_kind` checked single-value position on
//! the index and the slice bounds but not on the base, so `two()[0]` and
//! `two()[:]` were accepted while Go refuses them with
//! `multiple-value two() ... in single-value context`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn body(stmt: &str) -> String {
    format!(
        "package main\n\nfn two() ([]int, []int) {{\n    return []int{{1, 2}}, []int{{3, 4}}\n}}\n\nfn one() []int {{\n    return []int{{1, 2}}\n}}\n\nfn main() {{\n    {stmt}\n}}\n"
    )
}

fn assert_reject(stmt: &str) {
    let src = body(stmt);
    let r = check(&src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected REJECT for:\n{src}\n{:?}",
        r.diagnostics
    );
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&r)
    );
}

fn assert_accept(stmt: &str) {
    let src = body(stmt);
    let r = check(&src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected ACCEPT for:\n{src}\n{:?}",
        r.diagnostics
    );
}

#[test]
fn multi_value_call_as_index_base_is_rejected() {
    assert_reject("_ = two()[0]");
}

#[test]
fn multi_value_call_as_slice_base_is_rejected() {
    assert_reject("_ = two()[:]");
    assert_reject("_ = two()[0:1]");
}

#[test]
fn single_value_call_base_is_accepted() {
    assert_accept("_ = one()[0]");
    assert_accept("_ = one()[:]");
    assert_accept("_ = one()[0:1]");
}
