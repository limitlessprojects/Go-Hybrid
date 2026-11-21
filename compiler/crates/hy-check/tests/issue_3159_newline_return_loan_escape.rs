//! Regression for the checker-side consequence of issue #3159 (fix PR #4122).
//!
//! The parser fix stops a bare `return` from consuming the next line as its
//! value list. That AST change is pinned by `hy-syntax` unit tests, but its
//! security consequence was not: `Checker::process_return` runs the
//! named-result loan-escape check (`HYB-E3104`) only when `values.is_empty()`.
//! Before the fix this program parsed as `return fallback()`, so the loan
//! stored in the named result `out` was never inspected and `hyb check`
//! reported **0 diagnostics** — the exact O2 bypass described in #3159.
//!
//! These tests drive the checker end to end on the minimal shapes:
//!   1. a bare `return` followed by a next-line expression must still reject
//!      the escaping loan (`HYB-E3104`);
//!   2. the same explicit `return fallback()` on one line stays legal — the
//!      named result is overwritten, so nothing borrows the local `x`;
//!   3. a bare `return` at the end of the block keeps its pre-existing
//!      rejection (the control for the fix's no-regression claim).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const BYPASS: &str = r#"
type int_ptr = *int

fn fallback() int_ptr {
    var p int_ptr
    return p
}

fn compute() (out int_ptr) {
    x := 5
    out = &x
    return
    fallback()
}
"#;

const EXPLICIT_RETURN_CONTROL: &str = r#"
type int_ptr = *int

fn fallback() int_ptr {
    var p int_ptr
    return p
}

fn compute() (out int_ptr) {
    x := 5
    out = &x
    return fallback()
}
"#;

const BARE_RETURN_AT_END_CONTROL: &str = r#"
type int_ptr = *int

fn compute() (out int_ptr) {
    x := 5
    out = &x
    return
}
"#;

fn diagnostics(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("issue_3159.hy", src).unwrap_or_else(|diags| {
        panic!("fixture must parse after the #3159 fix: {diags:#?}");
    });
    let result = check_file(&ast);
    (
        result.verdict,
        result.diagnostics.iter().map(|d| d.code.clone()).collect(),
    )
}

#[test]
fn bare_return_before_next_line_expression_does_not_skip_loan_escape() {
    let (verdict, codes) = diagnostics(BYPASS);
    assert_eq!(
        verdict,
        Verdict::Reject,
        "a loan escaping through a named result must reject; got {codes:?}"
    );
    assert!(
        codes.iter().any(|code| code == "HYB-E3104"),
        "expected HYB-E3104 (O2 loan escape), got {codes:?}"
    );
}

#[test]
fn explicit_same_line_return_overwriting_the_named_result_is_legal() {
    let (verdict, codes) = diagnostics(EXPLICIT_RETURN_CONTROL);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "returning a fresh value on the same line leaves no escaping loan; got {codes:?}"
    );
    assert!(codes.is_empty(), "expected 0 diagnostics, got {codes:?}");
}

#[test]
fn bare_return_at_end_of_block_still_rejects_the_escaping_loan() {
    let (verdict, codes) = diagnostics(BARE_RETURN_AT_END_CONTROL);
    assert_eq!(verdict, Verdict::Reject, "expected a rejection, got {codes:?}");
    assert!(
        codes.iter().any(|code| code == "HYB-E3104"),
        "expected HYB-E3104 (O2 loan escape), got {codes:?}"
    );
}
