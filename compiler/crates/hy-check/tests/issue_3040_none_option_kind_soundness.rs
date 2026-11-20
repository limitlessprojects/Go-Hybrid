//! Regression tests for issue #3040:
//! hy-check: 'none' outside match is given OpKind::Unknown, letting invalid returns
//! (fn f() int { return none }), assignments, and operators pass check.
//!
//! Under Row 13 (BUILDABLE) / GUARANTEES.md, `none` represents an option zero value
//! and must evaluate to `OpKind::Option(Unknown)`. Returning `none` from a non-option function,
//! assigning `none` to scalar targets, passing `none` to non-option parameters, or applying
//! unary/binary operators to `none` must be rejected under `HYB-E1002`.
//! Returning `none` from a function declaring `option[T]` and matching on `none` must stay accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    (
        res.verdict,
        res.diagnostics.into_iter().map(|d| d.code).collect(),
    )
}

fn assert_reject_e1002(src: &str) {
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Reject,
        "expected Reject for:\n{src}\nGot diags: {diags:?}"
    );
    assert!(
        diags.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 in {diags:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\nGot diags: {diags:?}"
    );
    assert!(
        diags.is_empty(),
        "expected 0 diagnostics, got {diags:?} for:\n{src}"
    );
}

#[test]
fn reject_none_return_in_scalar_and_composite_functions() {
    assert_reject_e1002("fn f() int {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_reject_e1002("fn f() string {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_reject_e1002("fn f() bool {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_reject_e1002("fn f() float64 {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_reject_e1002("fn f() [2]int {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_reject_e1002("struct point { x int }\nfn f() point {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
}

#[test]
fn reject_none_in_scalar_assignments_and_var_decls() {
    assert_reject_e1002("fn main() {\n    var x int = none\n    _ = x\n}\n");
    assert_reject_e1002("fn main() {\n    var x int = 0\n    x = none\n    _ = x\n}\n");
    assert_reject_e1002("fn main() {\n    var s string = \"\"\n    s = none\n    _ = s\n}\n");
}

#[test]
fn reject_none_in_operators() {
    assert_reject_e1002("fn main() {\n    _ = !none\n}\n");
    assert_reject_e1002("fn main() {\n    _ = -none\n}\n");
    assert_reject_e1002("fn main() {\n    _ = none + 1\n}\n");
    assert_reject_e1002("fn main() {\n    _ = 1 + none\n}\n");
    assert_reject_e1002("fn main() {\n    _ = none < 1\n}\n");
}

#[test]
fn reject_none_passed_to_scalar_parameters() {
    assert_reject_e1002("fn take(x int) {}\nfn main() {\n    take(none)\n}\n");
    assert_reject_e1002("fn take_str(s string) {}\nfn main() {\n    take_str(none)\n}\n");
}

#[test]
fn accept_none_returned_from_option_functions() {
    assert_accept("fn f() option[int] {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_accept("fn f() option[string] {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
    assert_accept("fn f() option[bool] {\n    return none\n}\nfn main() {\n    _ = f()\n}\n");
}

#[test]
fn accept_none_in_match_arms() {
    assert_accept(
        "fn wrap() option[int] {\n    return none\n}\nfn main() {\n    match wrap() {\n        some(x): { _ = x }\n        none: {}\n    }\n}\n",
    );
}
